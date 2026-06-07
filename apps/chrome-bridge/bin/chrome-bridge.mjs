#!/usr/bin/env node
import { createHash } from 'node:crypto';
import http from 'node:http';
import net from 'node:net';
import { createReadStream, createWriteStream, existsSync, readFileSync } from 'node:fs';
import { copyFile, mkdir, mkdtemp, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { homedir, tmpdir } from 'node:os';
import { basename, delimiter, dirname, extname, isAbsolute, join, resolve } from 'node:path';
import { pipeline } from 'node:stream/promises';
import { fileURLToPath } from 'node:url';
import { spawn, spawnSync } from 'node:child_process';
import readline from 'node:readline';

import { chromium } from 'playwright-core';

const require = createRequire(import.meta.url);
const PLAYWRIGHT_VERSION = require('playwright-core/package.json').version;
const PROTOCOL_VERSION = 1;
const DEFAULT_CDP_HOST = '127.0.0.1';
const DEFAULT_CDP_PORT = 9224;
const MANAGED_CDP_MAX_PORT = 9234;
const LEGACY_LOCAL_CDP_PORT = 922;
const DEFAULT_PROFILE_DIR = join(homedir(), '.google-profile-automation-profile');
const DEFAULT_STATE_DIR = join(homedir(), '.google-profile-automation-state');
const DEFAULT_SOURCE_ARCHIVE_MODE = 'ai-source';
const DEFAULT_MAX_MINUTES = 30;
const DEFAULT_BROWSER_TIMEOUT_MS = 45000;
const DEFAULT_CDP_CONNECT_TIMEOUT_MS = 30000;
const DEFAULT_GLOBAL_MODAL_SWEEP_MS = 2500;
const DEFAULT_MESSAGE_STREAM_RETRY_LIMIT = 0;
const DEFAULT_MESSAGE_STREAM_RETRY_DELAY_MS = 10000;
const DEFAULT_ARTIFACT_STALL_REPAIR_SECONDS = 0;
const DEFAULT_ARTIFACT_REPAIR_ATTEMPT_LIMIT = 0;
const DEFAULT_ARTIFACT_CONVERSATION_RECOVERY_LIMIT = 3;

const args = parseArgs(process.argv.slice(2));
const shouldSelfTest = args.selfTest === 'true';
const globalConfig = loadGlobalJailgunConfig();

const cdpUrlSetting = firstSetting([
  ['--cdp-url', args.cdpUrl],
  ['JAILGUN_CDP_URL', process.env.JAILGUN_CDP_URL],
  ['~/.jailgun/config.json:cdp_url', globalConfig.cdpUrl],
]);
const cdpHostSetting = firstSetting([
  ['--cdp-host', args.cdpHost],
  ['--host', args.host],
  ['JAILGUN_CDP_HOST', process.env.JAILGUN_CDP_HOST],
  ['GOOGLE_AUTOMATION_REMOTE_DEBUG_HOST', process.env.GOOGLE_AUTOMATION_REMOTE_DEBUG_HOST],
  ['~/.jailgun/config.json:cdp_host', globalConfig.cdpHost],
]);
const cdpPortSetting = firstSetting([
  ['--cdp-port', args.cdpPort],
  ['--port', args.port],
  ['JAILGUN_CDP_PORT', process.env.JAILGUN_CDP_PORT],
  ['GOOGLE_AUTOMATION_REMOTE_DEBUG_PORT', process.env.GOOGLE_AUTOMATION_REMOTE_DEBUG_PORT],
  ['~/.jailgun/config.json:cdp_port', globalConfig.cdpPort],
]);
const cdpUrlOverride = cdpUrlSetting?.value ?? null;
const cdpHost = cdpHostSetting?.value ?? DEFAULT_CDP_HOST;
const cdpPort = numberFrom(cdpPortSetting?.value, DEFAULT_CDP_PORT);
const profileDir = resolvePath(args.profileDir ?? process.env.JAILGUN_CHROME_PROFILE_DIR ?? process.env.GOOGLE_AUTOMATION_PROFILE_DIR ?? globalConfig.profileDir ?? DEFAULT_PROFILE_DIR);
const stateDir = resolvePath(args.stateDir ?? process.env.JAILGUN_CHROME_STATE_DIR ?? process.env.GOOGLE_AUTOMATION_STATE_DIR ?? globalConfig.stateDir ?? DEFAULT_STATE_DIR);
const profilePoolSetting = firstSetting([
  ['--profile-pool', args.profilePool],
  ['JAILGUN_CHROME_PROFILE_POOL', process.env.JAILGUN_CHROME_PROFILE_POOL],
  ['JAILGUN_CHROME_PROFILE_DIRS', process.env.JAILGUN_CHROME_PROFILE_DIRS],
  ['~/.jailgun/config.json:profile_pool', globalConfig.profilePool],
]);
const profilePortsSetting = firstSetting([
  ['--profile-ports', args.profilePorts],
  ['JAILGUN_CHROME_PROFILE_PORTS', process.env.JAILGUN_CHROME_PROFILE_PORTS],
  ['~/.jailgun/config.json:profile_ports', globalConfig.profilePorts],
]);

const settings = {
  cdpUrl: cdpUrlOverride ?? `http://${cdpHost}:${cdpPort}`,
  cdpEndpointSource: cdpUrlSetting?.source ?? cdpPortSetting?.source ?? cdpHostSetting?.source ?? 'default',
  cdpEndpointConfigured: Boolean(cdpUrlSetting || cdpPortSetting || cdpHostSetting),
  profileDir,
  stateDir,
  profilePool: buildBrowserProfilePool({
    profilePoolValue: profilePoolSetting?.value,
    profilePoolSource: profilePoolSetting?.source,
    profilePortsValue: profilePortsSetting?.value,
    defaultProfileDir: profileDir,
    defaultStateDir: stateDir,
    baseCdpUrl: cdpUrlOverride ?? `http://${cdpHost}:${cdpPort}`,
    cdpEndpointSource: cdpUrlSetting?.source ?? cdpPortSetting?.source ?? cdpHostSetting?.source ?? 'default',
  }),
  profilePoolExplicit: Boolean(profilePoolSetting),
  chromeExecutable: args.chromeExecutable ?? args.browserExecutable ?? process.env.JAILGUN_CHROME_EXECUTABLE ?? process.env.GOOGLE_CHROME_EXECUTABLE ?? globalConfig.chromeExecutable ?? '',
  chromeHeadless: booleanFrom(
    args.headed === 'true'
      ? false
      : (args.headless ?? process.env.JAILGUN_CHROME_HEADLESS ?? process.env.GOOGLE_AUTOMATION_HEADLESS),
    false,
  ),
  browserTimeoutMs: numberFrom(args.browserTimeoutMs ?? args.timeoutMs ?? process.env.JAILGUN_CHROME_TIMEOUT_MS ?? process.env.GOOGLE_AUTOMATION_TIMEOUT_MS, DEFAULT_BROWSER_TIMEOUT_MS),
  downloadsDir: resolvePath(args.downloadsDir ?? process.env.JAILGUN_DOWNLOADS_DIR ?? join(homedir(), 'Downloads')),
  artifactsDir: resolvePath(args.artifactsDir ?? process.env.JAILGUN_ARTIFACTS_DIR ?? 'artifacts'),
  sourceMode: args.sourceMode ?? process.env.JAILGUN_SOURCE_ARCHIVE_MODE ?? DEFAULT_SOURCE_ARCHIVE_MODE,
  tarTargetName: args.tarTargetName ?? process.env.JAILGUN_TAR_TARGET_NAME ?? '',
  downloadTargetName: args.downloadTargetName ?? process.env.JAILGUN_DOWNLOAD_TARGET_NAME ?? '',
  submitDelaySeconds: numberFrom(args.submitDelaySeconds ?? process.env.JAILGUN_SUBMIT_DELAY_SECONDS, 0),
  submitJitterSeconds: numberFrom(args.submitJitterSeconds ?? process.env.JAILGUN_SUBMIT_JITTER_SECONDS, 0),
  tarWaitMinutes: numberFrom(args.tarWaitMinutes ?? process.env.JAILGUN_TAR_WAIT_MINUTES, DEFAULT_MAX_MINUTES),
  globalModalSweepMs: numberFrom(args.globalModalSweepMs ?? process.env.JAILGUN_GLOBAL_MODAL_SWEEP_MS, DEFAULT_GLOBAL_MODAL_SWEEP_MS),
  messageStreamRetryLimit: DEFAULT_MESSAGE_STREAM_RETRY_LIMIT,
  messageStreamRetryDelayMs: numberFrom(
    args.messageStreamRetryDelayMs ?? process.env.JAILGUN_MESSAGE_STREAM_RETRY_DELAY_MS,
    DEFAULT_MESSAGE_STREAM_RETRY_DELAY_MS,
  ),
  artifactStallRepairMs: Math.max(0, numberFrom(
    args.artifactStallRepairSeconds ?? process.env.JAILGUN_ARTIFACT_STALL_REPAIR_SECONDS,
    DEFAULT_ARTIFACT_STALL_REPAIR_SECONDS,
  ) * 1000),
  artifactRepairAttemptLimit: Math.max(0, Math.floor(numberFrom(
    args.artifactRepairAttempts
      ?? args.artifactRepairAttemptLimit
      ?? process.env.JAILGUN_ARTIFACT_REPAIR_ATTEMPTS
      ?? process.env.JAILGUN_ARTIFACT_REPAIR_ATTEMPT_LIMIT,
    DEFAULT_ARTIFACT_REPAIR_ATTEMPT_LIMIT,
  ))),
  artifactConversationRecoveryLimit: Math.max(0, Math.floor(numberFrom(
    args.artifactConversationRecoveryLimit ?? process.env.JAILGUN_ARTIFACT_CONVERSATION_RECOVERY_LIMIT,
    DEFAULT_ARTIFACT_CONVERSATION_RECOVERY_LIMIT,
  ))),
  recoverKnownRunTabs: booleanFrom(args.recoverKnownRunTabs ?? process.env.JAILGUN_RECOVER_KNOWN_RUN_TABS, true),
  knownRunArtifactsDir: resolvePath(args.knownRunArtifactsDir ?? process.env.JAILGUN_KNOWN_RUN_ARTIFACTS_DIR ?? join('artifacts', 'live-runs')),
};

class ChromeBridge {
  constructor(options) {
    this.options = options;
    this.browsers = new Map();
    this.profileSlots = new Map();
    for (const slot of options.profilePool) {
      this.profileSlots.set(slot.profileDir, slot);
    }
    this.dynamicProfileSlots = [];
    this.authPages = new Map();
    this.tabs = new Map();
    this.keepAliveTimers = new Map();
    this.shutdownRequested = false;
    this.globalDismissalTimer = null;
    this.globalDismissalRunning = false;
    this.lastEnvelope = null;
  }

  async run() {
    await mkdir(this.options.downloadsDir, { recursive: true });
    await mkdir(this.options.artifactsDir, { recursive: true });

    const rl = readline.createInterface({
      input: process.stdin,
      crlfDelay: Infinity,
    });

    rl.on('line', (line) => {
      void this.handleLine(line).catch((error) => {
        this.logError('dispatch-error', error);
      });
    });

    await new Promise((resolvePromise) => {
      rl.once('close', resolvePromise);
    });

    await this.shutdown('stdin-closed', 0, this.lastEnvelope);
  }

  async handleLine(line) {
    if (!line.trim()) {
      return;
    }
    let envelope;
    try {
      envelope = JSON.parse(line);
      validateEnvelope(envelope);
      this.lastEnvelope = envelope;
    } catch (error) {
      this.emitRaw({
        v: PROTOCOL_VERSION,
        type: 'error',
        run_id: 'unknown',
        ts: timestamp(),
        payload: errorPayload('protocol-error', error),
      });
      return;
    }

    const type = envelope.type;
    if (type === 'hello') {
      await this.handleHello(envelope);
      return;
    }
    if (type === 'ping') {
      this.emit(envelope, 'pong', {});
      return;
    }
    if (type === 'shutdown') {
      await this.shutdown('orchestrator-requested', envelope.payload?.drain_timeout_ms ?? 5000, envelope);
      return;
    }
    if (type.startsWith('auth-')) {
      await this.handleAuthCommand(envelope);
      return;
    }

    const tabId = requiredTabId(envelope);
    this.enqueue(tabId, async () => {
      switch (type) {
        case 'open-tab':
          await this.openTab(envelope);
          break;
        case 'upload-archive':
          await this.uploadArchive(envelope);
          break;
        case 'submit-prompt':
          await this.submitPrompt(envelope);
          break;
        case 'monitor-tab':
          await this.monitorTab(envelope);
          break;
        case 'stop-generation':
          await this.stopGeneration(envelope);
          break;
        case 'close-tab':
          await this.closeTab(envelope, 'orchestrator-requested');
          break;
        case 'approve-or-deny':
          await this.applyPromptPolicy(envelope);
          break;
        default:
          throw new Error(`unknown command type: ${type}`);
      }
    }, envelope);
  }

  async handleAuthCommand(envelope) {
    try {
      switch (envelope.type) {
        case 'auth-status':
          await this.authStatus(envelope);
          break;
        case 'auth-begin':
          await this.authBegin(envelope);
          break;
        case 'auth-select-email-code':
          await this.authSelectEmailCode(envelope);
          break;
        case 'auth-submit-code':
          await this.authSubmitCode(envelope);
          break;
        case 'auth-screenshot':
          await this.authScreenshot(envelope);
          break;
        case 'auth-cancel':
          await this.authCancel(envelope);
          break;
        default:
          throw new Error(`unknown auth command type: ${envelope.type}`);
      }
    } catch (error) {
      this.emit(envelope, 'auth-failed', {
        reason: redactSensitiveText(error?.message || String(error)),
        manual_browser_required: isManualBrowserRequiredError(error),
      }, undefined);
      if (!isManualBrowserRequiredError(error)) {
        this.emit(envelope, 'error', errorPayload('auth-command-failed', error), undefined);
      }
    }
  }

  enqueue(tabId, work, envelope) {
    const current = this.tabs.get(tabId) ?? { page: null, queue: Promise.resolve(), monitoring: false, failed: false };
    current.queue = current.queue
      .then(async () => {
        if (current.failed && envelope.type !== 'close-tab') {
          this.bridgeLog(envelope, 'tab-command-skip', 'skipped', 'skipping command after tab fatal error', {
            command: envelope.type,
          }, 'warn');
          return;
        }
        await work();
      })
      .catch((error) => {
        current.failed = true;
        this.bridgeLog(envelope, 'tab-command-failed', 'failed', error?.message || String(error), {
          command: envelope.type,
        }, 'error');
        this.emit(envelope, 'error', errorPayload('tab-command-failed', error), tabId);
      });
    this.tabs.set(tabId, current);
  }

  async handleHello(envelope) {
    try {
      const records = await this.ensureInitialBrowsers(envelope);
      const primary = records[0];
      this.emit(envelope, 'bridge-ready', {
        node_version: process.version,
        playwright_version: PLAYWRIGHT_VERSION,
        browser: 'chromium-cdp',
        browser_version: primary.browserVersion,
        cdp_url: primary.endpoint.cdpUrl,
        managed_chrome_started: records.some((record) => record.endpoint.started),
        profile_count: this.options.profilePool.length,
        profiles: records.map((record) => browserProfileState(record)),
        capabilities: [
          'managed-chrome',
          'managed-profile-pool',
          'auth-status',
          'auth-email-code',
          'source-upload',
          'prompt-submit-readiness',
          'tar-capture',
          'rate-limit-detection',
          'global-modal-sweeper',
          'known-run-tab-recovery',
        ],
      });
      await this.recoverKnownRunChatGptTabs(envelope, 'startup-known-run-tab-recovery');
      await this.sweepAllChatGptModals(envelope, 'startup-global-modal-sweep');
      this.startGlobalDismissalSweep(envelope);
    } catch (error) {
      this.logError('startup-failed', error);
      this.emit(envelope, 'error', errorPayload('bridge-startup-failed', error));
      await this.shutdown('bridge-startup-failed', 0);
      process.exitCode = 1;
      setImmediate(() => process.exit(1));
    }
  }

  async ensureInitialBrowsers(envelope = null) {
    const slots = this.options.profilePoolExplicit ? this.options.profilePool : [this.options.profilePool[0]];
    const records = [];
    for (const slot of slots) {
      records.push(await this.ensureBrowser(envelope, slot));
    }
    return records;
  }

  async ensureBrowser(envelope = null, slot = this.options.profilePool[0]) {
    const existing = this.browsers.get(slot.key);
    if (existing?.browser && existing?.context) {
      return existing;
    }
    const logStartup = envelope
      ? (phase, status, message, fields, level) => this.bridgeLog(envelope, phase, status, message, {
        ...browserSlotLogFields(slot),
        ...fields,
      }, level)
      : null;
    const browserOptions = {
      ...this.options,
      cdpUrl: slot.cdpUrl,
      cdpEndpointSource: slot.cdpEndpointSource,
      profileDir: slot.profileDir,
      profileName: slot.profileName,
      stateDir: slot.stateDir,
    };
    let chrome = null;
    let browser = null;
    const connectTimeoutMs = Math.min(this.options.browserTimeoutMs, DEFAULT_CDP_CONNECT_TIMEOUT_MS);
    for (let attempt = 1; attempt <= 2; attempt += 1) {
      chrome = await ensureManagedChromeRunning(browserOptions, logStartup);
      logStartup?.('browser-connect', 'starting', 'connecting Playwright over CDP', {
        cdp_url: chrome.cdpUrl,
        browser_timeout_ms: String(connectTimeoutMs),
        attempt: String(attempt),
      });
      try {
        browser = await chromium.connectOverCDP(chrome.cdpUrl, { timeout: connectTimeoutMs });
        logStartup?.('browser-connect', 'ok', 'connected Playwright over CDP', {
          cdp_url: chrome.cdpUrl,
          browser_timeout_ms: String(connectTimeoutMs),
          attempt: String(attempt),
        });
        break;
      } catch (error) {
        if (attempt >= 2 || !isRetryableCdpConnectError(error)) {
          throw error;
        }
        const restart = await restartManagedBrowserForConnectFailure(browserOptions, chrome, error, logStartup);
        logStartup?.('browser-connect', restart.status === 'failed' ? 'restart-failed' : 'retrying', 'retrying Playwright CDP connection after managed browser restart', {
          cdp_url: chrome.cdpUrl,
          browser_timeout_ms: String(connectTimeoutMs),
          attempt: String(attempt),
          next_attempt: String(attempt + 1),
          restart_status: restart.status,
          restart_error: restart.error || '',
          reason: error?.message || String(error),
        }, restart.status === 'failed' ? 'error' : 'warn');
      }
    }
    if (!browser || !chrome) {
      throw new Error(`failed to connect Playwright over CDP for profile ${slot.profileName}`);
    }
    const context = browser.contexts()[0];
    if (!context) {
      throw new Error(`no browser context found at ${chrome.cdpUrl}`);
    }
    const record = {
      slot,
      browser,
      context,
      endpoint: {
        ...chrome,
        profileName: slot.profileName,
        profileDir: slot.profileDir,
        stateDir: slot.stateDir,
      },
      browserVersion: await browser.version(),
    };
    this.browsers.set(slot.key, record);
    await writeManagedBrowserPoolState(this.options.stateDir, this.activeBrowserStates());
    return record;
  }

  async openTab(envelope) {
    const slot = this.selectBrowserSlot(envelope);
    const record = await this.ensureBrowser(envelope, slot);
    const tabId = requiredTabId(envelope);
    const payload = envelope.payload ?? {};
    const page = await record.context.newPage();
    page.on('dialog', async (dialog) => {
      const message = dialog.message();
      this.bridgeLog(envelope, 'native-dialog', 'detected', 'browser dialog detected', {
        type: dialog.type(),
        message: compact(message, 180),
      }, 'warn');
      if (dialog.type() === 'beforeunload') {
        await dialog.dismiss().catch(() => undefined);
      } else {
        await dialog.accept().catch(() => undefined);
      }
    });
    await page.goto(payload.chat_url || 'https://chatgpt.com/', {
      waitUntil: 'domcontentloaded',
      timeout: 60000,
    });
    await page.bringToFront();
    const current = this.tabs.get(tabId) ?? { queue: Promise.resolve(), monitoring: false };
    this.tabs.set(tabId, {
      ...current,
      page,
      monitoring: false,
      failed: false,
      browserSlot: slot.slot,
      browserProfile: slot.profileName,
      browserProfileDir: slot.profileDir,
      browserCdpUrl: record.endpoint.cdpUrl,
    });
    this.startKeepAlive(`tab:${tabId}`, page, envelope, 'tab-keep-alive');
    this.bridgeLog(envelope, 'open-tab', 'ok', 'tab opened', {
      page_url: page.url(),
      model: payload.model || '',
      ...browserSlotLogFields(slot, record.endpoint.cdpUrl),
    });
    this.emit(envelope, 'tab-opened', {
      page_url: page.url(),
      page_id: `tab-${String(tabId).padStart(2, '0')}`,
      browser_profile: slot.profileName,
      browser_profile_dir: slot.profileDir,
      browser_slot: slot.slot,
      cdp_url: record.endpoint.cdpUrl,
    }, tabId);
  }

  async uploadArchive(envelope) {
    const tab = this.requireTab(envelope);
    const payload = envelope.payload ?? {};
    const prompt = payload.prompt || null;
    const localArchivePath = payload.local_archive_path ? resolvePath(String(payload.local_archive_path)) : '';
    this.bridgeLog(envelope, 'source-upload', 'started', 'preparing source archive', {
      repo_url: payload.repo_url || '',
      ref_name: payload.ref_name || 'HEAD',
      fresh_source_clone: String(Boolean(payload.fresh_source_clone)),
      local_archive_path: localArchivePath ? '[provided]' : '',
      prompt_bundled: String(Boolean(prompt)),
    });
    const archive = await createSourceArchive({
      repoUrl: localArchivePath ? '' : requiredString(payload.repo_url, 'repo_url'),
      refName: payload.ref_name || 'HEAD',
      prefix: payload.prefix || 'source/',
      archiveFilename: payload.archive_filename || 'source.tar.gz',
      tmpParent: payload.tmp_parent || undefined,
      mode: this.options.sourceMode,
      freshSourceClone: Boolean(payload.fresh_source_clone),
      localArchivePath,
    });

    let deletedTemp = false;
    try {
      await uploadFileToChat(tab.page, archive.archivePath, payload.timeout_ms ?? 45000);

      // Fill prompt into composer immediately — while upload is still processing.
      // The send button will be disabled until the upload completes.
      if (prompt) {
        const composer = await waitForChatComposer(tab.page, payload.submit_timeout_ms ?? 45000, {
          log: (phase, status, message, fields = {}, level = 'info') => {
            this.bridgeLog(envelope, phase, status, message, fields, level);
          },
          authState: (state) => this.emitPromptAuthState(envelope, state),
        });
        await composer.fill(prompt, { timeout: payload.submit_timeout_ms ?? 45000 });
        this.bridgeLog(envelope, 'prompt-injected-during-upload', 'ok', 'prompt text filled into composer while upload is processing', {
          char_count: String(prompt.length),
        });
      }

      const uploadConfirmed = await confirmUpload(
        tab.page,
        archive.archiveFilename,
        payload.confirm_selectors ?? [],
        payload.timeout_ms ?? 45000,
      );
      if (!uploadConfirmed) {
        this.bridgeLog(envelope, 'source-upload', 'warn', 'upload confirmation was not visible; continuing with prompt submission', {
          archive_filename: archive.archiveFilename,
          fresh_source_clone: String(archive.freshSourceClone),
          clone_dir: archive.cloneDir,
        }, 'warn');
      }
      const fileStat = await stat(archive.archivePath);
      const sha256 = await sha256File(archive.archivePath);
      if (payload.delete_after_upload !== false && archive.tempRoot) {
        await rm(archive.tempRoot, { recursive: true, force: true });
        deletedTemp = true;
      }
      this.emit(envelope, 'archive-uploaded', {
        sha256,
        size_bytes: fileStat.size,
        commit: archive.commit,
        archive_filename: archive.archiveFilename,
        deleted_temp: deletedTemp,
        fresh_source_clone: archive.freshSourceClone,
        clone_dir: archive.cloneDir,
      });
      this.bridgeLog(envelope, 'source-upload', 'ok', 'source archive uploaded', {
        sha256,
        size_bytes: String(fileStat.size),
        archive_filename: archive.archiveFilename,
        fresh_source_clone: String(archive.freshSourceClone),
        clone_dir: archive.cloneDir,
      });

      // Submit the prompt now — the upload is confirmed, send button should become enabled.
      if (prompt) {
        await this.runDismissals(tab.page, envelope, 'prompt-submit-preflight');
        const result = await submitPromptToChat(tab.page, prompt, payload.submit_timeout_ms ?? 45000, {
          dismiss: async (phase) => this.runDismissals(tab.page, envelope, phase),
          log: (phase, status, message, fields = {}, level = 'info') => {
            this.bridgeLog(envelope, phase, status, message, fields, level);
          },
          authState: (state) => this.emitPromptAuthState(envelope, state),
        });
        this.emit(envelope, 'prompt-submitted', {
          char_count: prompt.length,
        });
        this.bridgeLog(envelope, 'prompt-submitted', 'ok', 'prompt accepted by ChatGPT (bundled with upload)', {
          char_count: String(prompt.length),
          acceptance_reason: result.acceptanceReason || '',
        });
      }
    } finally {
      if (!deletedTemp) {
        await rm(archive.tempRoot, { recursive: true, force: true }).catch(() => undefined);
      }
    }
  }

  async submitPrompt(envelope) {
    const tab = this.requireTab(envelope);
    const payload = envelope.payload ?? {};
    const prompt = requiredString(payload.prompt, 'prompt');
    await this.runDismissals(tab.page, envelope, 'prompt-submit-preflight');
    const result = await submitPromptToChat(tab.page, prompt, payload.submit_timeout_ms ?? 45000, {
      dismiss: async (phase) => this.runDismissals(tab.page, envelope, phase),
      log: (phase, status, message, fields = {}, level = 'info') => {
        this.bridgeLog(envelope, phase, status, message, fields, level);
      },
      authState: (state) => this.emitPromptAuthState(envelope, state),
    });
    this.emit(envelope, 'prompt-submitted', {
      char_count: prompt.length,
    });
    this.bridgeLog(envelope, 'prompt-submitted', 'ok', 'prompt accepted by ChatGPT', {
      char_count: String(prompt.length),
      acceptance_reason: result.acceptanceReason || '',
    });
  }

  async monitorTab(envelope) {
    const tab = this.requireTab(envelope);
    if (tab.monitoring) {
      return;
    }
    tab.monitoring = true;
    const tabId = requiredTabId(envelope);
    const payload = envelope.payload ?? {};
    const pollMs = Math.max(1000, payload.telemetry_tick_ms ?? 10000);
    const completionMs = Math.max(1000, payload.completion_check_ms ?? 2000);
    const startedAt = Date.now();
    const deadline = startedAt + Math.max(1, this.options.tarWaitMinutes) * 60000;
    const outputDir = join(this.options.downloadsDir, envelope.run_id, `tab-${String(tabId).padStart(2, '0')}`);
    const targetName = artifactTargetName(this.options);
    const waitingForTex = isTexNameLike(targetName);
    const artifactLabel = artifactWaitLabel(targetName);
    const noArtifactKind = (kind) => waitingForTex ? kind.replace(/-no-tar$/, '-no-artifact') : kind;
    await mkdir(outputDir, { recursive: true });
    let lastTelemetry = 0;
    let tick = 0;
    let messageStreamRetries = 0;
    let lastProgressSignature = '';
    let lastProgressChangedAt = startedAt;
    const artifactRepairState = {
      attempts: 0,
      submitted: false,
      lastError: '',
    };
    const artifactConversationRecoveryState = {
      attempts: 0,
      visitedUrls: new Set([normalizeChatGptUrl(tab.page.url())].filter(Boolean)),
    };
    const failNoTar = async (kind, message, details = {}) => {
      const recovery = await recoverArtifactConversationDownload(this, tab, envelope, {
        kind,
        message,
        outputDir,
        tabId,
        targetName,
        state: artifactConversationRecoveryState,
      });
      if (recovery.downloaded) {
        return;
      }
      await emitNoTarErrorAndCleanup(
        this,
        tab,
        envelope,
        kind,
        message,
        {
          ...details,
          ...artifactConversationRecoveryDetails(recovery),
        },
      );
    };
    this.bridgeLog(envelope, 'monitor-started', 'ok', 'tab monitor loop started', {
      completion_check_ms: String(completionMs),
      telemetry_tick_ms: String(pollMs),
      deadline: new Date(deadline).toISOString(),
      page_url: tab.page.url(),
    });

    while (!this.shutdownRequested && Date.now() <= deadline) {
      tick += 1;
      let discovery;
      let status;
      try {
        await this.runDismissals(tab.page, envelope, 'monitor-dismissals');
        await this.handleGitHubToolPrompts(tab.page, envelope);
        discovery = await discoverTarCandidates(tab.page, targetName);
        status = await readGenerationStatus(tab.page);
      } catch (error) {
        if (isTransientNavigationError(error)) {
          this.bridgeLog(envelope, 'monitor-navigation-retry', 'retrying', 'page navigated during monitor check; retrying after load', {
            reason: error?.message || String(error),
            page_url: tab.page.url(),
          }, 'warn');
          await tab.page.waitForLoadState('domcontentloaded', { timeout: 2000 }).catch(() => undefined);
          await sleep(Math.min(completionMs, 500));
          continue;
        }
        throw error;
      }
      const ranked = rankCandidates(discovery.candidates, targetName);
      const now = Date.now();
      const progressSignature = [
        ranked.length,
        Boolean(status.activeStop),
        status.finalActions,
        Boolean(status.messageStreamError),
        discovery.lastTextLength,
        compact(discovery.lastTextPreview || '', 240),
      ].join('|');
      if (progressSignature !== lastProgressSignature) {
        lastProgressSignature = progressSignature;
        lastProgressChangedAt = now;
      }
      const stalledMs = Math.max(0, now - lastProgressChangedAt);
      const progressKind = now - lastTelemetry >= pollMs ? 'telemetry' : 'completion-check';
      if (progressKind === 'telemetry') {
        lastTelemetry = now;
      }
      this.emit(envelope, 'tab-progress', {
        kind: progressKind,
        phase: ranked.length > 0 ? 'artifact-candidate-found' : status.messageStreamError ? 'message-stream-error' : status.activeStop ? 'generating' : 'checking',
        busy_reason: status.activeStop ? 'active-stop-button' : status.messageStreamError ? 'message-stream-error' : null,
        has_active_stop: Boolean(status.activeStop),
        has_final_actions: status.finalActions > 0,
        last_text_length: discovery.lastTextLength,
        page_url: tab.page.url(),
      });
      if (progressKind === 'telemetry') {
        this.bridgeLog(envelope, 'monitor-poll', 'running', 'tab monitor telemetry', {
          tick: String(tick),
          candidate_count: String(ranked.length),
          scanned_control_count: String(discovery.scannedControlCount ?? 0),
          assistant_roots: String(discovery.assistantRootCount ?? 0),
          has_active_stop: String(Boolean(status.activeStop)),
          has_final_actions: String(status.finalActions > 0),
          message_stream_error: String(Boolean(status.messageStreamError)),
          retry_available: String(Boolean(status.retryAvailable)),
          message_stream_retries: String(messageStreamRetries),
          last_text_length: String(discovery.lastTextLength),
          stalled_ms: String(stalledMs),
          preview: compact(discovery.lastTextPreview || '', 160),
          page_url: tab.page.url(),
        });
      }

      if (ranked.length > 0) {
        const candidate = ranked[0];
        const candidateTargetName = downloadTargetNameForCandidate(this.options, candidate, targetName);
        this.emit(envelope, 'tar-discovered', {
          candidates: ranked.slice(0, 5),
          selected_index: candidate.index,
          file_kind: candidateFileKind(candidate, candidateTargetName),
        });
        const preStop = await stopIfGenerating(tab.page).catch((error) => ({
          clicked: false,
          reason: `error:${error?.message || String(error)}`,
        }));
        const preStopMethod = preStop.clicked
          ? (preStop.label || 'button')
          : `not-active:${preStop.reason || 'not-found'}`;
        this.emit(envelope, 'generation-stopped', {
          method: preStopMethod,
          phase: 'pre-download',
        });
        this.bridgeLog(
          envelope,
          'generation-stopped',
          preStop.clicked ? 'ok' : 'not-active',
          preStop.clicked
            ? 'stopped generation pre-download'
            : 'generation not active pre-download',
          { method: preStopMethod, phase: 'pre-download' },
        );
        const startedDownloadAt = timestamp();
        const targetPath = join(outputDir, candidateTargetName ? normalizeArtifactName(candidateTargetName) : normalizeArtifactName(candidate.label || candidate.download || candidate.href || 'chatgpt-output.tar.gz'));
        this.emit(envelope, 'download-started', {
          candidate_index: candidate.index,
          remote_url: candidate.href || '',
          target_path: targetPath,
          started_at: startedDownloadAt,
        });
        this.bridgeLog(envelope, 'download-started', 'started', 'clicking selected artifact download candidate', {
          candidate_index: String(candidate.index),
          candidate_count: String(ranked.length),
          candidate_score: String(candidate.score ?? ''),
          target_path: targetPath,
          target_name: candidateTargetName,
          label: compact(candidate.label || candidate.download || candidate.href || '', 160),
        });
        let completePayload = null;
        let cleanup = null;
        try {
          await this.runDismissals(tab.page, envelope, 'download-preflight');
          const file = await downloadCandidate(tab.page, candidate, outputDir, 120000, {
            bridge: this,
            envelope,
            tabId,
            artifactsDir: this.options.artifactsDir,
            page: tab.page,
            targetName: candidateTargetName,
            downloadsDir: outputDir,
            browserProfileDir: tab.browserProfileDir,
          });
          const receiptPath = join(this.options.artifactsDir, 'receipts', envelope.run_id, `tab-${String(tabId).padStart(2, '0')}-download.json`);
          await mkdir(resolve(receiptPath, '..'), { recursive: true });
          const finishedDownloadAt = timestamp();
          const downloadLatencyMs = Math.max(0, Date.parse(finishedDownloadAt) - Date.parse(startedDownloadAt)) || 0;
          completePayload = {
            sha256: file.sha256,
            size_bytes: file.sizeBytes,
            local_path: file.path,
            receipt_path: receiptPath,
            original_name: file.suggested,
            local_name: file.localName,
            file_kind: file.fileKind,
            artifact_kind: file.artifactKind,
            validation_status: file.validationStatus,
            discovery_strategy: file.persistedBy === 'materialized-from-text' ? 'materialized_from_text' : 'browser_download',
            download_url: candidate.href || null,
            entry_count: file.entryCount,
            started_at: startedDownloadAt,
            finished_at: finishedDownloadAt,
            download_latency_ms: downloadLatencyMs,
          };
          await writeFile(receiptPath, JSON.stringify(completePayload, null, 2));
        } catch (error) {
          const failureBundlePath = error?.failureBundlePath || await writeDownloadFailureBundle({
            bridge: this,
            envelope,
            tabId,
            artifactsDir: this.options.artifactsDir,
            page: tab.page,
          }, {
            kind: 'download-failed',
            message: error?.message || String(error),
            output_dir: outputDir,
            candidate: downloadCandidateMetadata(candidate),
            attempts: [],
            error: error?.message || String(error),
            created_at: timestamp(),
            details: {
              target_path: targetPath,
              candidate_index: String(candidate.index),
              candidate_count: String(ranked.length),
              candidate_score: String(candidate.score ?? ''),
            },
          });
          await emitDownloadErrorAndCleanup(this, tab, envelope, error, {
            target_path: targetPath,
            candidate_index: String(candidate.index),
            candidate_count: String(ranked.length),
            candidate_score: String(candidate.score ?? ''),
            target_name: candidateTargetName,
            file_kind: candidateFileKind(candidate, candidateTargetName),
            failure_bundle_path: failureBundlePath,
            failed_download_bundle_path: failureBundlePath,
          });
          return;
        }
        cleanup = await finalizeTabAfterDownload(this, tab, envelope, 'download-complete');
        this.emit(envelope, 'download-complete', completePayload);
        this.bridgeLog(envelope, 'download-complete', 'ok', 'download receipt written and tab closed', {
          sha256: completePayload.sha256,
          size_bytes: String(completePayload.size_bytes),
          entry_count: String(completePayload.entry_count ?? ''),
          file_kind: completePayload.file_kind,
          receipt_path: completePayload.receipt_path,
          local_path: completePayload.local_path,
          generation_stop_method: cleanup?.stopMethod || '',
          tab_closed: String(Boolean(cleanup?.closed)),
          cleanup_errors: (cleanup?.errors || []).join(';'),
        });
        if (!cleanup?.closed || cleanup.errors.length > 0) {
          throw new Error(`download completed but tab cleanup failed for tab ${tabId}: ${(cleanup?.errors || ['tab-not-closed']).join('; ')}`);
        }
        return;
      }

      if (
        this.options.artifactStallRepairMs > 0
        && ranked.length === 0
        && status.activeStop
        && !status.messageStreamError
        && discovery.assistantRootCount > 0
        && discovery.lastTextLength > 0
        && discovery.lastTextLength < 1000
        && stalledMs >= this.options.artifactStallRepairMs
      ) {
        await failNoTar(
          noArtifactKind('artifact-stall-no-tar'),
          `assistant stalled without ${artifactLabel} download candidate`,
          {
            artifact_repair_attempts: String(artifactRepairState.attempts),
            artifact_repair_max_attempts: String(this.options.artifactRepairAttemptLimit),
            artifact_repair_submitted: String(Boolean(artifactRepairState.submitted)),
            artifact_repair_error: artifactRepairState.lastError,
            artifact_repair_disabled: 'true',
            stalled_ms: String(stalledMs),
            threshold_ms: String(this.options.artifactStallRepairMs),
          },
        );
        return;
      }

      if (status.messageStreamError) {
        await failNoTar(
          noArtifactKind('message-stream-no-tar'),
          `assistant hit message stream error without ${artifactLabel} after ${messageStreamRetries} retry attempts`,
        );
        return;
      }

      // Handle A/B feedback: select longest response if both are done
      if (discovery.abFeedbackActive && !status.activeStop && ranked.length === 0) {
        const abResult = await selectLongestABResponse(tab.page);
        if (abResult.detected) {
          this.bridgeLog(envelope, 'ab-feedback-detected', abResult.selected ? 'selected' : 'detected', abResult.selected ? 'selected longest A/B response' : 'A/B feedback detected but could not select response', {
            selected_index: String(abResult.selectedIndex),
            response_lengths: JSON.stringify(abResult.responseLengths),
            selected: String(abResult.selected),
            reason: abResult.reason || '',
          });
          // After selecting, re-scan for tar candidates in the selected response
          if (abResult.selected) {
            await sleep(1000);
            continue;  // Re-enter the loop to re-scan tar candidates
          }
        }
      }

      if (!status.activeStop && status.finalActions > 0) {
        const materialized = await materializeVisibleTextArtifact(tab.page, targetName, outputDir, {
          bridge: this,
          envelope,
          tabId,
        });
        if (materialized) {
          const receiptPath = join(this.options.artifactsDir, 'receipts', envelope.run_id, `tab-${String(tabId).padStart(2, '0')}-download.json`);
          await mkdir(resolve(receiptPath, '..'), { recursive: true });
          const finishedAt = timestamp();
          const completePayload = {
            sha256: materialized.sha256,
            size_bytes: materialized.sizeBytes,
            local_path: materialized.path,
            receipt_path: receiptPath,
            original_name: materialized.suggested,
            local_name: materialized.localName,
            file_kind: materialized.fileKind,
            artifact_kind: materialized.artifactKind,
            validation_status: materialized.validationStatus,
            discovery_strategy: 'materialized_from_text',
            download_url: null,
            entry_count: materialized.entryCount,
            started_at: finishedAt,
            finished_at: finishedAt,
            download_latency_ms: 0,
            materialized_from_text: true,
          };
          await writeFile(receiptPath, JSON.stringify(completePayload, null, 2));
          const cleanup = await finalizeTabAfterDownload(this, tab, envelope, 'download-complete');
          this.emit(envelope, 'download-complete', completePayload);
          this.bridgeLog(envelope, 'download-complete', 'ok', 'materialized text-safe artifact and closed tab', {
            sha256: completePayload.sha256,
            size_bytes: String(completePayload.size_bytes),
            file_kind: completePayload.file_kind,
            artifact_kind: completePayload.artifact_kind,
            validation_status: completePayload.validation_status,
            receipt_path: completePayload.receipt_path,
            local_path: completePayload.local_path,
            discovery_strategy: completePayload.discovery_strategy,
            generation_stop_method: cleanup?.stopMethod || '',
            tab_closed: String(Boolean(cleanup?.closed)),
            cleanup_errors: (cleanup?.errors || []).join(';'),
          });
          if (!cleanup?.closed || cleanup.errors.length > 0) {
            throw new Error(`text artifact materialized but tab cleanup failed for tab ${tabId}: ${(cleanup?.errors || ['tab-not-closed']).join('; ')}`);
          }
          return;
        }
        const repair = await submitArtifactRepairIfNeeded(this, tab, envelope, targetName, artifactRepairState, discovery);
        if (repair.submitted) {
          lastProgressSignature = '';
          lastProgressChangedAt = Date.now();
          await sleep(Math.min(completionMs, 1000));
          continue;
        }
        await failNoTar(
          noArtifactKind('done-no-tar'),
          `assistant finished but no ${artifactLabel} download candidate was found`,
          {
            artifact_repair_attempts: String(artifactRepairState.attempts),
            artifact_repair_submitted: String(Boolean(artifactRepairState.submitted)),
            artifact_repair_error: artifactRepairState.lastError,
          },
        );
        return;
      }

      await sleep(Math.min(completionMs, pollMs));
    }

    await failNoTar(
      noArtifactKind('timeout-no-tar'),
      `timed out after ${this.options.tarWaitMinutes} minutes waiting for ${artifactLabel} download candidate`,
      {
        artifact_repair_attempts: String(artifactRepairState.attempts),
        artifact_repair_submitted: String(Boolean(artifactRepairState.submitted)),
        artifact_repair_error: artifactRepairState.lastError,
      },
    );
  }

  async runDismissals(page, envelope, phase) {
    const popup = await dismissPopups(page);
    if (popup.detected) {
      this.bridgeLog(envelope, 'popup-dismissal', popup.clicked ? 'clicked' : 'detected', popup.clicked ? 'dismissed popup' : 'popup detected without safe click', {
        source_phase: phase,
        kind: popup.kind || '',
        label: popup.label || '',
        reason: popup.reason || '',
        excerpt: compact(popup.excerpt || '', 200),
      }, popup.clicked ? 'info' : 'warn');
      if (popup.kind === 'session-expired') {
        const reason = popup.reason || 'session expired prompt detected';
        this.emit(envelope, 'session-expired', {
          page_url: page.url(),
          reason,
        });
        throw new Error(reason);
      }
    }

    const rateLimit = await dismissRateLimitModal(page);
    if (rateLimit.detected) {
      this.emit(envelope, 'rate-limit-detected', {
        dismissed: Boolean(rateLimit.dismissed),
        excerpt: rateLimit.excerpt || '',
      });
      this.bridgeLog(envelope, 'rate-limit-detected', rateLimit.dismissed ? 'clicked' : 'detected', rateLimit.dismissed ? 'dismissed rate-limit modal' : 'rate-limit modal detected without safe click', {
        source_phase: phase,
        button_label: rateLimit.buttonLabel || '',
        reason: rateLimit.reason || '',
        excerpt: compact(rateLimit.excerpt || '', 200),
      }, 'warn');
    }
  }

  startGlobalDismissalSweep(envelope) {
    if (this.globalDismissalTimer || this.options.globalModalSweepMs <= 0) {
      return;
    }
    this.globalDismissalTimer = setInterval(() => {
      void this.sweepAllChatGptModals(envelope, 'global-modal-sweep').catch((error) => {
        this.logError('global-modal-sweep', error);
      });
    }, this.options.globalModalSweepMs);
    this.globalDismissalTimer.unref?.();
    this.bridgeLog(envelope, 'global-modal-sweep', 'started', 'global ChatGPT modal sweeper started', {
      interval_ms: String(this.options.globalModalSweepMs),
      max_expected_latency_ms: String(this.options.globalModalSweepMs),
    });
  }

  async sweepAllChatGptModals(envelope, phase) {
    if (this.browsers.size === 0 || this.globalDismissalRunning) {
      return { pages: 0, dismissed: 0 };
    }
    this.globalDismissalRunning = true;
    let pages = 0;
    let dismissed = 0;
    try {
      for (const record of this.activeBrowserRecords()) {
        for (const page of record.context.pages()) {
          if (!page || page.isClosed() || !isChatGptPageUrl(page.url())) {
            continue;
          }
          pages += 1;
          const tabId = this.tabIdForPage(page);
          const rateLimit = await dismissRateLimitModal(page);
          if (!rateLimit.detected) {
            continue;
          }
          if (rateLimit.dismissed) {
            dismissed += 1;
          }
          const orphan = tabId == null;
          this.emit(envelope, 'rate-limit-detected', {
            dismissed: Boolean(rateLimit.dismissed),
            excerpt: rateLimit.excerpt || '',
            page_url: page.url(),
            source_phase: phase,
            global_sweep: true,
            orphan,
            browser_profile: record.slot.profileName,
            browser_profile_dir: record.slot.profileDir,
            browser_slot: record.slot.slot,
            cdp_url: record.endpoint.cdpUrl,
          }, tabId ?? undefined);
          this.bridgeLog(
            envelope,
            'global-rate-limit-sweep',
            rateLimit.dismissed ? 'clicked' : 'detected',
            rateLimit.dismissed ? 'dismissed rate-limit modal from global sweep' : 'rate-limit modal detected without safe click in global sweep',
            {
              source_phase: phase,
              page_url: page.url(),
              tab_id: tabId == null ? '' : String(tabId),
              orphan: String(orphan),
              button_label: rateLimit.buttonLabel || '',
              reason: rateLimit.reason || '',
              excerpt: compact(rateLimit.excerpt || '', 200),
              ...browserSlotLogFields(record.slot, record.endpoint.cdpUrl),
            },
            'warn',
          );
        }
      }
      return { pages, dismissed };
    } finally {
      this.globalDismissalRunning = false;
    }
  }

  tabIdForPage(page) {
    for (const [tabId, tab] of this.tabs.entries()) {
      if (tab.page === page) {
        return tabId;
      }
    }
    return null;
  }

  async recoverKnownRunChatGptTabs(envelope, phase) {
    if (this.browsers.size === 0 || !this.options.recoverKnownRunTabs) {
      return { scanned: 0, matched: 0, closed: 0, downloaded: 0 };
    }
    const known = await collectKnownRunChatGptUrls(this.options.knownRunArtifactsDir, envelope.run_id);
    if (known.size === 0) {
      this.bridgeLog(envelope, phase, 'skipped', 'no known prior run ChatGPT URLs found for recovery', {
        artifacts_dir: this.options.knownRunArtifactsDir,
      });
      return { scanned: 0, matched: 0, closed: 0, downloaded: 0 };
    }
    let scanned = 0;
    let matched = 0;
    let closed = 0;
    let downloaded = 0;
    for (const record of this.activeBrowserRecords()) {
      for (const page of record.context.pages()) {
        if (!page || page.isClosed() || !isChatGptPageUrl(page.url())) {
          continue;
        }
        scanned += 1;
        const normalized = normalizeChatGptUrl(page.url());
        const source = known.get(normalized);
        if (!source) {
          continue;
        }
        matched += 1;
        const summary = await recoverKnownRunPage(this, page, envelope, source, phase);
        if (summary.closed) {
          closed += 1;
        }
        if (summary.downloaded) {
          downloaded += 1;
        }
      }
    }
    this.bridgeLog(envelope, phase, 'done', 'known prior run ChatGPT tab recovery finished', {
      artifacts_dir: this.options.knownRunArtifactsDir,
      scanned: String(scanned),
      matched: String(matched),
      closed: String(closed),
      downloaded: String(downloaded),
    });
    return { scanned, matched, closed, downloaded };
  }

  async handleGitHubToolPrompts(page, envelope) {
    const result = await handleGitHubToolPrompt(page);
    if (!result.detected) {
      return;
    }
    this.emit(envelope, 'tool-prompt-detected', {
      candidate: result.candidate,
    });
    this.emit(envelope, 'prompt-policy-applied', {
      signature: result.candidate?.signature || '',
      decision: result.decision || 'deny',
      clicked: Boolean(result.clicked),
      reason: result.reason || null,
    });
    this.bridgeLog(envelope, 'github-tool-prompt', result.clicked ? 'clicked' : 'detected', result.clicked ? 'clicked GitHub prompt policy control' : 'GitHub prompt detected without click', {
      decision: result.decision || 'deny',
      clicked: String(Boolean(result.clicked)),
      label: result.candidate?.label || '',
      repository: result.candidate?.repository || '',
      reason: result.reason || '',
    }, result.clicked ? 'info' : 'warn');
  }

  async applyPromptPolicy(envelope) {
    const tab = this.requireTab(envelope);
    const result = await clickPolicyControlBySignature(tab.page, envelope.payload ?? {});
    this.emit(envelope, 'prompt-policy-applied', {
      signature: envelope.payload?.signature ?? '',
      decision: envelope.payload?.decision ?? 'unknown',
      clicked: Boolean(result.clicked),
      reason: result.reason || null,
    });
    this.bridgeLog(envelope, 'prompt-policy-command', result.clicked ? 'clicked' : 'not-clicked', 'applied prompt policy command', {
      decision: envelope.payload?.decision ?? 'unknown',
      signature: envelope.payload?.signature ?? '',
      reason: result.reason || '',
      label: result.label || '',
    }, result.clicked ? 'info' : 'warn');
  }

  async authStatus(envelope) {
    const page = await this.authPage(envelope, envelope.payload?.chat_url || 'https://chatgpt.com/');
    const state = await detectChatAuthState(page);
    this.emitAuthState(envelope, state);
    if (state.state === 'ready') {
      this.emit(envelope, 'auth-complete', {
        page_url: state.pageUrl,
        composer_detected: true,
      }, undefined);
    } else if (state.state === 'session-expired') {
      this.emit(envelope, 'session-expired', {
        page_url: state.pageUrl,
        reason: state.reason || 'session expired',
      }, undefined);
    }
  }

  async authBegin(envelope) {
    const payload = envelope.payload ?? {};
    const page = await this.authPage(envelope, payload.chat_url || 'https://chatgpt.com/');
    let state = await detectChatAuthState(page);
    this.emitAuthState(envelope, state);
    if (state.state === 'ready') {
      this.emit(envelope, 'auth-complete', {
        page_url: state.pageUrl,
        composer_detected: true,
      }, undefined);
      return;
    }
    if (state.manualAction) {
      this.emit(envelope, 'auth-action-needed', state.manualAction, undefined);
      throw manualBrowserRequired(state.manualAction.reason);
    }

    const emailHint = String(payload.email_hint || '').trim();
    if (emailHint) {
      await fillKnownEmailIfPresent(page, emailHint);
      state = await detectChatAuthState(page);
      this.emitAuthState(envelope, state);
      if (state.state === 'ready') {
        this.emit(envelope, 'auth-complete', {
          page_url: state.pageUrl,
          composer_detected: true,
        }, undefined);
        return;
      }
      if (state.manualAction) {
        this.emit(envelope, 'auth-action-needed', state.manualAction, undefined);
        throw manualBrowserRequired(state.manualAction.reason);
      }
    }

    if (payload.prefer_email_code) {
      const selected = await selectEmailCodeControl(page);
      if (selected.clicked) {
        this.emit(envelope, 'auth-code-requested', {
          channel: 'email',
          destination_hint: selected.destinationHint || null,
        }, undefined);
        this.emitAuthState(envelope, await detectChatAuthState(page));
        return;
      }
    }

    this.emit(envelope, 'auth-action-needed', {
      action: 'manual-browser-required',
      reason: 'no safe email-code control was detected',
    }, undefined);
    throw manualBrowserRequired('no safe email-code control was detected');
  }

  async authSelectEmailCode(envelope) {
    const page = await this.existingAuthPage(envelope);
    const selected = await selectEmailCodeControl(page);
    if (!selected.clicked) {
      this.emit(envelope, 'auth-action-needed', {
        action: 'manual-browser-required',
        reason: selected.reason || 'no safe email-code control was detected',
      }, undefined);
      throw manualBrowserRequired(selected.reason || 'no safe email-code control was detected');
    }
    this.emit(envelope, 'auth-code-requested', {
      channel: 'email',
      destination_hint: selected.destinationHint || null,
    }, undefined);
  }

  async authSubmitCode(envelope) {
    const page = await this.existingAuthPage(envelope);
    const code = String(envelope.payload?.code || '').trim();
    if (!/^[0-9A-Za-z][0-9A-Za-z -]{3,31}$/.test(code)) {
      throw new Error('verification code format was not accepted');
    }
    await submitVerificationCode(page, code);
    this.emit(envelope, 'auth-code-submitted', { accepted: true }, undefined);
    const state = await waitForAuthReadyOrAction(page, this.options.browserTimeoutMs);
    this.emitAuthState(envelope, state);
    if (state.state === 'ready') {
      this.emit(envelope, 'auth-complete', {
        page_url: state.pageUrl,
        composer_detected: true,
      }, undefined);
      return;
    }
    if (state.manualAction) {
      this.emit(envelope, 'auth-action-needed', state.manualAction, undefined);
      throw manualBrowserRequired(state.manualAction.reason);
    }
    throw new Error(state.reason || 'auth code was submitted but ChatGPT composer was not verified');
  }

  async authScreenshot(envelope) {
    const page = await this.existingAuthPage(envelope);
    const target = resolvePath(requiredString(envelope.payload?.path, 'path'));
    await mkdir(dirname(target), { recursive: true });
    await page.screenshot({ path: target, fullPage: true });
    this.bridgeLog(envelope, 'auth-screenshot', 'ok', 'auth screenshot written', {
      path: target,
    });
  }

  async authCancel(envelope) {
    const slot = this.selectAuthSlot(envelope);
    this.clearKeepAlive(`auth:${slot.key}`);
    const page = this.authPages.get(slot.key);
    if (page && !page.isClosed()) {
      await page.close().catch(() => undefined);
    }
    this.authPages.delete(slot.key);
    this.bridgeLog(envelope, 'auth-cancel', 'ok', 'auth flow cancelled', browserSlotLogFields(slot));
  }

  async authPage(envelope, chatUrl) {
    const slot = this.selectAuthSlot(envelope);
    const record = await this.ensureBrowser(envelope, slot);
    let page = this.authPages.get(slot.key);
    if (!page || page.isClosed()) {
      page = await record.context.newPage();
      this.authPages.set(slot.key, page);
      this.startKeepAlive(`auth:${slot.key}`, page, envelope, 'auth-keep-alive');
    }
    await page.goto(chatUrl, { waitUntil: 'domcontentloaded', timeout: 60000 });
    await page.bringToFront();
    return page;
  }

  async existingAuthPage(envelope) {
    const slot = this.selectAuthSlot(envelope);
    const page = this.authPages.get(slot.key);
    if (!page || page.isClosed()) {
      throw new Error('auth flow has not been started for this browser profile');
    }
    await page.bringToFront().catch(() => undefined);
    return page;
  }

  selectAuthSlot(envelope) {
    const profileDir = envelope.payload?.profile_dir ? resolvePath(String(envelope.payload.profile_dir)) : '';
    if (profileDir) {
      const exact = findProfileSlotByDir([...this.profileSlots.values()], profileDir);
      if (exact) {
        return exact;
      }
      return this.dynamicSlotForProfileDir(profileDir);
    }
    if (this.options.profilePool.length !== 1) {
      throw manualBrowserRequired('auth command requires profile_dir when multiple browser profiles are configured');
    }
    return this.options.profilePool[0];
  }

  emitAuthState(envelope, state) {
    this.emit(envelope, 'auth-state', {
      state: state.state,
      page_url: state.pageUrl,
      reason: state.reason || null,
      composer_detected: Boolean(state.composerDetected),
      code_requested: Boolean(state.codeRequested),
    }, undefined);
  }

  emitPromptAuthState(envelope, state) {
    this.emitAuthState(envelope, state);
    const tabId = requiredTabId(envelope);
    if (state.state === 'session-expired') {
      this.emit(envelope, 'session-expired', {
        page_url: state.pageUrl,
        reason: state.reason || 'session expired',
      }, tabId);
    } else if (state.manualAction) {
      this.emit(envelope, 'auth-action-needed', state.manualAction, tabId);
    } else if (state.state === 'auth-required' || state.state === 'code-requested') {
      this.emit(envelope, 'auth-failed', {
        reason: state.reason || state.state,
        manual_browser_required: false,
      }, tabId);
    }
  }

  async stopGeneration(envelope) {
    const tab = this.requireTab(envelope);
    const result = await stopIfGenerating(tab.page);
    if (result.clicked) {
      this.emit(envelope, 'generation-stopped', { method: result.label || 'button', phase: 'commanded' });
    }
  }

  async closeTab(envelope, reason) {
    const tab = this.requireTab(envelope);
    const pageUrl = tab.page.url();
    this.clearKeepAlive(`tab:${requiredTabId(envelope)}`);
    await tab.page.close({ runBeforeUnload: Boolean(envelope.payload?.run_before_unload) }).catch(() => undefined);
    this.emit(envelope, 'tab-closed', {
      page_url: pageUrl,
      reason,
    });
  }

  async closeTabAfterReceipt(tab, envelope, reason) {
    if (!tab.page || tab.page.isClosed()) {
      return false;
    }
    const pageUrl = tab.page.url();
    this.clearKeepAlive(`tab:${requiredTabId(envelope)}`);
    await tab.page.close({ runBeforeUnload: false }).catch(() => undefined);
    this.emit(envelope, 'tab-closed', {
      page_url: pageUrl,
      reason,
    });
    tab.page = null;
    return true;
  }

  startKeepAlive(key, page, envelope, phase, intervalMs = 60000) {
    if (!key || !page) {
      return;
    }
    this.clearKeepAlive(key);
    const timer = setInterval(() => {
      void this.pingKeepAlive(key, page, envelope, phase).catch((error) => {
        this.bridgeLog(envelope, phase, 'failed', 'keep-alive ping failed', {
          reason: error?.message || String(error),
        }, 'warn');
      });
    }, intervalMs);
    timer.unref?.();
    this.keepAliveTimers.set(key, timer);
  }

  async pingKeepAlive(key, page, envelope, phase) {
    if (!page || page.isClosed()) {
      this.clearKeepAlive(key);
      return;
    }
    try {
      await page.evaluate(() => 0);
    } catch (error) {
      const message = error?.message || String(error);
      if (/Target closed|has been closed|Session closed|browser has been closed/i.test(message)) {
        this.clearKeepAlive(key);
        return;
      }
      throw error;
    }
  }

  clearKeepAlive(key) {
    const timer = this.keepAliveTimers.get(key);
    if (!timer) {
      return;
    }
    clearInterval(timer);
    this.keepAliveTimers.delete(key);
  }

  clearAllKeepAlives() {
    for (const key of [...this.keepAliveTimers.keys()]) {
      this.clearKeepAlive(key);
    }
  }

  requireTab(envelope) {
    const tabId = requiredTabId(envelope);
    const tab = this.tabs.get(tabId);
    if (!tab?.page) {
      throw new Error(`tab ${tabId} is not open`);
    }
    return tab;
  }

  selectBrowserSlot(envelope) {
    const tabId = requiredTabId(envelope);
    const payloadProfileDir = envelope.payload?.profile_dir
      ? resolvePath(String(envelope.payload.profile_dir))
      : '';
    if (payloadProfileDir) {
      const exact = findProfileSlotByDir([...this.profileSlots.values()], payloadProfileDir);
      if (exact) {
        return exact;
      }
    }
    if (this.options.profilePoolExplicit && this.options.profilePool.length > 1) {
      return profilePoolSlotForTab(this.options.profilePool, tabId);
    }
    if (payloadProfileDir) {
      return this.dynamicSlotForProfileDir(payloadProfileDir);
    }
    return this.options.profilePool[0];
  }

  dynamicSlotForProfileDir(profileDir) {
    const existing = this.profileSlots.get(profileDir);
    if (existing) {
      return existing;
    }
    const index = this.options.profilePool.length + this.dynamicProfileSlots.length;
    const slot = createBrowserProfileSlot({
      index,
      entry: profileDir,
      defaultStateDir: this.options.stateDir,
      baseEndpoint: parseCdpEndpoint(this.options.cdpUrl),
      cdpEndpointSource: 'open-tab.profile_dir',
      poolSize: index + 1,
      explicit: false,
    });
    this.dynamicProfileSlots.push(slot);
    this.profileSlots.set(slot.profileDir, slot);
    return slot;
  }

  activeBrowserStates() {
    return [...this.browsers.values()].map((record) => browserProfileState(record));
  }

  activeBrowserRecords() {
    return [...this.browsers.values()];
  }

  async shutdown(reason, drainTimeoutMs, envelope = null) {
    if (this.shutdownRequested) {
      return;
    }
    this.shutdownRequested = true;
    if (drainTimeoutMs > 0) {
      await Promise.race([
        Promise.all([...this.tabs.values()].map((tab) => tab.queue?.catch(() => undefined))),
        sleep(drainTimeoutMs),
      ]).catch(() => undefined);
    }
    if (this.globalDismissalTimer) {
      clearInterval(this.globalDismissalTimer);
      this.globalDismissalTimer = null;
    }
    this.clearAllKeepAlives();
    await this.sweepAllChatGptModals(envelope ?? systemEnvelope(reason), 'shutdown-global-modal-sweep').catch(() => undefined);
    for (const [tabId, tab] of this.tabs.entries()) {
      if (tab.page && !tab.page.isClosed()) {
        const pageUrl = tab.page.url();
        if (envelope) {
          const stop = await stopIfGenerating(tab.page).catch((error) => ({
            clicked: false,
            reason: `shutdown-stop-failed:${error?.message || String(error)}`,
          }));
          this.emit(envelope, 'generation-stopped', {
            method: stop.clicked ? (stop.label || 'button') : `shutdown-not-active:${stop.reason || 'not-found'}`,
            phase: 'shutdown',
          }, tabId);
        }
        await tab.page.close().catch(() => undefined);
        if (envelope) {
          this.emit(envelope, 'tab-closed', {
            page_url: pageUrl,
            reason,
          }, tabId);
        }
      }
    }
    if (envelope) {
      this.emit(envelope, 'bridge-shutting-down', { reason }, undefined);
    }
    const managedRecords = [...this.browsers.values()].filter((record) => managedBrowserRecordIsTerminable(record));
    for (const record of managedRecords) {
      record.endpoint.browserClose = await requestManagedBrowserClose(record).catch((error) => ({
        status: 'failed',
        sent: false,
        error: error?.message || String(error),
      }));
    }
    for (const record of this.browsers.values()) {
      await record.browser?.close().catch(() => undefined);
    }
    for (const record of managedRecords) {
      const result = await terminateManagedBrowserProcess(record.endpoint).catch((error) => ({
        status: 'failed',
        pid: record.endpoint.pid,
        cdp_url: record.endpoint.cdpUrl,
        error: error?.message || String(error),
      }));
      result.browser_close_sent = String(Boolean(record.endpoint.browserClose?.sent));
      result.browser_close_status = record.endpoint.browserClose?.status ?? '';
      result.browser_close_error = record.endpoint.browserClose?.error ?? '';
      record.endpoint.lastTermination = result;
      if (result.status === 'ok' || result.status === 'already-exited') {
        await writeManagedBrowserStoppedState(record.endpoint.stateDir, record.endpoint, result).catch(() => undefined);
        record.endpoint.pid = null;
        record.endpoint.started = false;
      }
      if (envelope) {
        this.bridgeLog(
          envelope,
          'managed-chrome-shutdown',
          result.status === 'ok' || result.status === 'already-exited' ? 'ok' : 'failed',
          result.status === 'ok' || result.status === 'already-exited'
            ? 'managed Chrome process stopped'
            : 'managed Chrome process cleanup failed',
          {
            pid: String(result.pid ?? record.endpoint.pid ?? ''),
            cdp_url: record.endpoint.cdpUrl,
            profile_dir: record.endpoint.profileDir,
            sigterm_sent: String(Boolean(result.sigterm_sent)),
            sigkill_sent: String(Boolean(result.sigkill_sent)),
            port_closed: String(Boolean(result.port_closed)),
            browser_close_sent: String(Boolean(record.endpoint.browserClose?.sent)),
            browser_close_status: record.endpoint.browserClose?.status ?? '',
            error: result.error || '',
          },
          result.status === 'ok' || result.status === 'already-exited' ? 'info' : 'error',
        );
      }
    }
    await writeManagedBrowserPoolState(this.options.stateDir, this.activeBrowserStates()).catch(() => undefined);
  }

  emit(envelope, type, payload, tabId = envelope?.tab_id) {
    this.emitRaw({
      v: PROTOCOL_VERSION,
      type,
      correlation_id: envelope?.id ?? undefined,
      run_id: envelope?.run_id ?? 'unknown',
      tab_id: tabId ?? undefined,
      ts: timestamp(),
      payload,
    });
  }

  emitRaw(envelope) {
    process.stdout.write(`${JSON.stringify(envelope)}\n`);
  }

  bridgeLog(envelope, phase, status, message, fields = {}, level = 'info') {
    const { redactedMessage, normalizedFields } = normalizeBridgeLogPayload(
      this.profileFieldsForEnvelope(envelope),
      fields,
      message,
      status,
    );
    this.emit(envelope, 'bridge-log', {
      level,
      phase,
      message: redactedMessage,
      fields: normalizedFields,
    });
    process.stderr.write(formatBridgeStderr(envelope, phase, status, redactedMessage, normalizedFields, level));
  }

  profileFieldsForEnvelope(envelope) {
    const tabId = envelope?.tab_id;
    if (!Number.isInteger(tabId)) {
      return {};
    }
    const tab = this.tabs.get(tabId);
    if (!tab?.browserProfile) {
      return {};
    }
    return {
      browser_slot: tab.browserSlot,
      browser_profile: tab.browserProfile,
      browser_profile_dir: tab.browserProfileDir,
      cdp_url: tab.browserCdpUrl,
    };
  }

  logError(phase, error) {
    process.stderr.write(`[chrome-bridge] ${phase}: ${redactSensitiveText(error?.stack || error?.message || String(error))}\n`);
  }
}

function buildBrowserProfilePool({
  profilePoolValue,
  profilePoolSource,
  profilePortsValue,
  defaultProfileDir,
  defaultStateDir,
  baseCdpUrl,
  cdpEndpointSource,
}) {
  const baseEndpoint = parseCdpEndpoint(baseCdpUrl);
  const entries = parseProfilePoolEntries(profilePoolValue);
  const effectiveEntries = entries.length > 0 ? entries : [defaultProfileDir];
  const profilePorts = parseProfilePortEntries(profilePortsValue);
  if (effectiveEntries.length > 1 && !isLocalCdpHost(baseEndpoint.hostname)) {
    throw new Error('managed Chrome profile pools require a local CDP host; use 127.0.0.1 or localhost');
  }
  return effectiveEntries.map((entry, index) => createBrowserProfileSlot({
    index,
    entry,
    profilePorts,
    defaultStateDir,
    baseEndpoint,
    cdpEndpointSource: profilePoolSource ?? cdpEndpointSource ?? 'default',
    poolSize: effectiveEntries.length,
    explicit: entries.length > 0,
  }));
}

function parseProfilePoolEntries(value) {
  if (!value || !String(value).trim()) {
    return [];
  }
  return String(value)
    .split(delimiter)
    .map((entry) => entry.trim())
    .filter(Boolean);
}

function parseProfilePortEntries(value) {
  const ports = new Map();
  if (!value || !String(value).trim()) {
    return ports;
  }
  for (const rawEntry of String(value).split(delimiter).map((entry) => entry.trim()).filter(Boolean)) {
    const eq = rawEntry.indexOf('=');
    if (eq <= 0) {
      throw new Error(`profile port entry must be name=port: ${rawEntry}`);
    }
    const name = rawEntry.slice(0, eq).trim();
    const port = Number(rawEntry.slice(eq + 1).trim());
    if (!name) {
      throw new Error(`profile port entry has an empty name: ${rawEntry}`);
    }
    if (!Number.isInteger(port) || port <= 0 || port > 65535) {
      throw new Error(`profile port entry has an invalid port: ${rawEntry}`);
    }
    ports.set(safeProfileName(name, ports.size), port);
  }
  return ports;
}

function createBrowserProfileSlot({
  index,
  entry,
  profilePorts,
  defaultStateDir,
  baseEndpoint,
  cdpEndpointSource,
  poolSize,
  explicit,
}) {
  const { name, profileDir } = parseProfilePoolEntry(entry, index);
  const safeName = safeProfileName(name || basename(profileDir) || `profile-${index + 1}`, index);
  const slot = index + 1;
  const stateDir = poolSize === 1 && !explicit
    ? defaultStateDir
    : join(defaultStateDir, 'profiles', safeName);
  const endpoint = cdpEndpointWithPort(baseEndpoint, profilePorts.get(safeName) ?? baseEndpoint.port + index);
  return {
    key: `${slot}:${profileDir}`,
    slot,
    profileName: safeName,
    profileDir,
    stateDir,
    cdpUrl: endpoint.origin,
    cdpEndpointSource,
  };
}

function profilePoolSlotForTab(profilePool, tabId) {
  if (!Array.isArray(profilePool) || profilePool.length === 0) {
    throw new Error('browser profile pool is empty');
  }
  return profilePool[(tabId - 1) % profilePool.length];
}

function findProfileSlotByDir(profilePool, profileDir) {
  return profilePool.find((slot) => slot.profileDir === resolvePath(profileDir)) ?? null;
}

function parseProfilePoolEntry(entry, index) {
  const trimmed = String(entry || '').trim();
  const eq = trimmed.indexOf('=');
  if (eq > 0) {
    const name = trimmed.slice(0, eq).trim();
    const value = trimmed.slice(eq + 1).trim();
    if (!value) {
      throw new Error(`profile pool entry ${index + 1} has an empty profile dir`);
    }
    return { name, profileDir: resolvePath(value) };
  }
  if (!trimmed) {
    throw new Error(`profile pool entry ${index + 1} is empty`);
  }
  return { name: '', profileDir: resolvePath(trimmed) };
}

function safeProfileName(value, index) {
  const normalized = String(value || '')
    .replace(/[^A-Za-z0-9_.-]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 48);
  return normalized || `profile-${index + 1}`;
}

function cdpEndpointWithPort(endpoint, port) {
  if (!Number.isInteger(port) || port <= 0 || port > 65535) {
    throw new Error(`managed Chrome profile pool exhausted CDP ports at ${port}`);
  }
  const host = endpoint.hostname.includes(':') && !endpoint.hostname.startsWith('[')
    ? `[${endpoint.hostname}]`
    : endpoint.hostname;
  return parseCdpEndpoint(`${endpoint.protocol}//${host}:${port}`);
}

function browserSlotLogFields(slot, cdpUrl = slot.cdpUrl) {
  return {
    browser_slot: String(slot.slot),
    browser_profile: slot.profileName,
    browser_profile_dir: slot.profileDir,
    cdp_url: cdpUrl,
  };
}

function browserProfileState(record) {
  return {
    slot: record.slot.slot,
    profile_name: record.slot.profileName,
    profile_dir: record.slot.profileDir,
    state_dir: record.slot.stateDir,
    cdp_url: record.endpoint.cdpUrl,
    pid: record.endpoint.pid ?? null,
    started: Boolean(record.endpoint.started),
    browser_version: record.browserVersion,
    updated_at: timestamp(),
  };
}

async function ensureManagedChromeRunning(options, logStartup = null) {
  const requestedEndpoint = parseCdpEndpoint(options.cdpUrl);
  const requestedProbe = await probeCdpEndpoint(requestedEndpoint, 750);
  const managedProbes = needsManagedCdpRecovery(requestedEndpoint, requestedProbe)
    ? await probeManagedCdpCandidates(750)
    : null;
  const startupPlan = planCdpEndpointRecovery(requestedEndpoint, requestedProbe, managedProbes);
  if (startupPlan.recovery) {
    logStartup?.('cdp-recovery', startupPlan.fatal ? 'failed' : 'redirected', startupPlan.fatal
      ? 'local Chrome CDP port 922 is not usable and no managed Jailgun Chrome port is available'
      : 'local Chrome CDP port 922 is not usable; switching to managed Jailgun Chrome', {
      ...cdpRecoveryLogFields(startupPlan.recovery),
      cdp_endpoint_source: options.cdpEndpointSource || 'unknown',
      cdp_endpoint_configured: String(Boolean(options.cdpEndpointConfigured)),
    }, startupPlan.fatal ? 'error' : 'warn');
  }
  if (startupPlan.fatal) {
    throw cdpRecoveryError(startupPlan.fatal);
  }

  const endpoint = startupPlan.endpoint;
  const probe = startupPlan.probe ?? requestedProbe;
  if (probe.status === 'cdp') {
    return { cdpUrl: endpoint.origin, started: false, pid: null };
  }
  if (probe.status === 'open-non-cdp') {
    throw new Error(`Port ${endpoint.hostname}:${endpoint.port} is open, but it is not responding as Chrome CDP at ${endpoint.origin}/json/version`);
  }

  if (!isLocalCdpHost(endpoint.hostname)) {
    throw new Error(`Chrome CDP is unreachable at ${endpoint.origin}, and chrome-bridge only auto-starts local Chrome endpoints`);
  }

  const executable = resolveChromeExecutable(options.chromeExecutable);
  await mkdir(options.profileDir, { recursive: true });
  await mkdir(options.stateDir, { recursive: true });
  await clearProfileLockArtifacts(options.profileDir);

  const displayState = await startManagedDisplayIfNeeded();

  const launchArgs = buildManagedChromeLaunchArgs(options, endpoint);
  const chromeEnv = {
    ...process.env,
    ...(displayState.display ? { DISPLAY: displayState.display } : {}),
  };
  const child = spawn(executable, launchArgs, {
    detached: true,
    stdio: 'ignore',
    env: chromeEnv,
  });
  child.unref();

  await writeManagedBrowserState(options.stateDir, {
    pid: child.pid,
    host: endpoint.hostname,
    port: endpoint.port,
    profileDir: options.profileDir,
    profileName: options.profileName ?? '',
    stateDir: options.stateDir,
    cdpUrl: endpoint.origin,
    executable,
    headless: false,
    display: displayState.display ?? '',
    xvfbPid: displayState.pid ?? null,
    startedAt: timestamp(),
  });

  try {
    await waitForCdpVersion(endpoint, options.browserTimeoutMs);
  } catch (error) {
    const lockArtifacts = detectProfileLockArtifacts(options.profileDir);
    if (lockArtifacts.length > 0) {
      await clearProfileLockArtifacts(options.profileDir).catch(() => undefined);
      error.message += `\nThe managed Chrome profile appears to be locked. Close any regular Chrome window using this profile and retry.\nProfile lock hints:\n- ${lockArtifacts.join('\n- ')}`;
    }
    if (displayState.pid) {
      await stopManagedDisplay(displayState.pid).catch(() => undefined);
    }
    throw error;
  }

  return { cdpUrl: endpoint.origin, started: true, pid: child.pid, child, xvfbPid: displayState.pid ?? null };
}

async function restartManagedBrowserForConnectFailure(options, chrome, error, logStartup = null) {
  const state = await readManagedBrowserState(options.stateDir).catch(() => null);
  const endpoint = {
    pid: chrome?.pid ?? state?.pid ?? null,
    cdpUrl: chrome?.cdpUrl ?? state?.cdpUrl ?? options.cdpUrl,
    profileDir: options.profileDir,
    profileName: options.profileName ?? state?.profileName ?? '',
    stateDir: options.stateDir,
    display: state?.display ?? '',
    xvfbPid: chrome?.xvfbPid ?? state?.xvfbPid ?? null,
  };
  logStartup?.('managed-chrome-restart', 'starting', 'restarting managed Chrome after CDP connect failure', {
    cdp_url: endpoint.cdpUrl,
    pid: String(endpoint.pid ?? ''),
    reason: error?.message || String(error),
  }, 'warn');
  const termination = await terminateManagedBrowserProcess(endpoint).catch((terminationError) => ({
    status: 'failed',
    pid: endpoint.pid,
    cdp_url: endpoint.cdpUrl,
    error: terminationError?.message || String(terminationError),
  }));
  if (termination.status === 'ok' || termination.status === 'already-exited' || termination.status === 'skipped') {
    await writeManagedBrowserStoppedState(options.stateDir, endpoint, termination).catch(() => undefined);
  }
  await clearProfileLockArtifacts(options.profileDir).catch(() => undefined);
  return termination;
}

async function readManagedBrowserState(stateDir) {
  const payload = await readFile(join(stateDir, 'managed-browser.json'), 'utf8');
  return JSON.parse(payload);
}

function isRetryableCdpConnectError(error) {
  const message = error?.message || String(error);
  return /connectOverCDP|Timeout|timed out|ECONNREFUSED|ECONNRESET|socket hang up|WebSocket is not open|Target page, context or browser has been closed|browser has been closed|Target closed/i.test(message);
}

function managedBrowserRecordIsTerminable(record) {
  const pid = Number(record?.endpoint?.pid);
  return Boolean(record?.endpoint?.started) && Number.isInteger(pid) && pid > 0;
}

async function requestManagedBrowserClose(record, timeoutMs = 1500) {
  const browser = record?.browser;
  if (!browser || typeof browser.newBrowserCDPSession !== 'function') {
    return {
      status: 'skipped',
      sent: false,
      error: 'missing browser CDP session',
    };
  }
  try {
    await Promise.race([
      (async () => {
        const session = await browser.newBrowserCDPSession();
        await session.send('Browser.close');
      })(),
      sleep(timeoutMs).then(() => {
        throw new Error(`Browser.close timed out after ${timeoutMs}ms`);
      }),
    ]);
    return {
      status: 'sent',
      sent: true,
      error: '',
    };
  } catch (error) {
    const message = error?.message || String(error);
    if (/Target closed|has been closed|disconnected|WebSocket is not open/i.test(message)) {
      return {
        status: 'sent-close-observed',
        sent: true,
        error: message,
      };
    }
    return {
      status: 'failed',
      sent: false,
      error: message,
    };
  }
}

async function terminateManagedBrowserProcess(endpoint, hooks = {}) {
  const pid = Number(endpoint?.pid);
  const cdpUrl = endpoint?.cdpUrl || '';
  if (!Number.isInteger(pid) || pid <= 0) {
    return {
      status: 'skipped',
      pid: endpoint?.pid ?? null,
      cdp_url: cdpUrl,
      sigterm_sent: false,
      sigkill_sent: false,
      port_closed: false,
      error: 'missing managed browser pid',
    };
  }
  const parsedEndpoint = cdpUrl ? parseCdpEndpoint(cdpUrl) : null;
  const xvfbPid = Number(endpoint?.xvfbPid);
  const killProcess = hooks.killProcess ?? ((targetPid, signal) => process.kill(targetPid, signal));
  const isProcessAliveFn = hooks.isProcessAlive ?? ((targetPid) => isProcessAlive(targetPid, killProcess));
  const isPortOpenFn = hooks.isPortOpen ?? ((host, port, timeoutMs) => isPortOpen(host, port, timeoutMs));
  const sleepFn = hooks.sleep ?? sleep;
  const timeoutMs = Math.max(0, hooks.timeoutMs ?? 3000);
  const intervalMs = Math.max(25, hooks.intervalMs ?? 100);
  const result = {
    status: 'ok',
    pid,
    cdp_url: cdpUrl,
    sigterm_sent: false,
    sigkill_sent: false,
    port_closed: false,
    error: '',
  };

  if (!isProcessAliveFn(pid)) {
    result.status = 'already-exited';
    result.port_closed = parsedEndpoint
      ? !(await isPortOpenFn(parsedEndpoint.hostname, parsedEndpoint.port, 250).catch(() => true))
      : false;
    return result;
  }

  try {
    killProcess(pid, 'SIGTERM');
    result.sigterm_sent = true;
  } catch (error) {
    if (error?.code === 'ESRCH') {
      result.status = 'already-exited';
      return result;
    }
    result.status = 'failed';
    result.error = error?.message || String(error);
    return result;
  }

  const afterTerm = await waitForManagedBrowserToStop({
    pid,
    endpoint: parsedEndpoint,
    timeoutMs,
    intervalMs,
    isProcessAlive: isProcessAliveFn,
    isPortOpen: isPortOpenFn,
    sleep: sleepFn,
  });
  result.port_closed = afterTerm.portClosed;
  if (!afterTerm.alive) {
    return result;
  }

  try {
    killProcess(pid, 'SIGKILL');
    result.sigkill_sent = true;
  } catch (error) {
    if (error?.code === 'ESRCH') {
      result.port_closed = parsedEndpoint
        ? !(await isPortOpenFn(parsedEndpoint.hostname, parsedEndpoint.port, 250).catch(() => true))
        : result.port_closed;
      return result;
    }
    result.status = 'failed';
    result.error = error?.message || String(error);
    return result;
  }

  const afterKill = await waitForManagedBrowserToStop({
    pid,
    endpoint: parsedEndpoint,
    timeoutMs: Math.min(timeoutMs, 1500),
    intervalMs,
    isProcessAlive: isProcessAliveFn,
    isPortOpen: isPortOpenFn,
    sleep: sleepFn,
  });
  result.port_closed = afterKill.portClosed;
  if (afterKill.alive) {
    result.status = 'failed';
    result.error = 'managed browser pid remained alive after SIGKILL';
  }
  if (Number.isInteger(xvfbPid) && xvfbPid > 0) {
    try {
      killProcess(xvfbPid, 'SIGTERM');
      await waitForManagedPidToExit(xvfbPid, 1500);
    } catch (error) {
      if (error?.code !== 'ESRCH') {
        result.status = 'failed';
        result.error = result.error || error?.message || String(error);
      }
    }
  }
  return result;
}

async function waitForManagedBrowserToStop({
  pid,
  endpoint,
  timeoutMs,
  intervalMs,
  isProcessAlive,
  isPortOpen,
  sleep: sleepFn,
}) {
  const deadline = Date.now() + timeoutMs;
  let alive = isProcessAlive(pid);
  let portClosed = endpoint
    ? !(await isPortOpen(endpoint.hostname, endpoint.port, 250).catch(() => true))
    : false;
  while (alive && !portClosed && Date.now() < deadline) {
    await sleepFn(Math.min(intervalMs, Math.max(0, deadline - Date.now())));
    alive = isProcessAlive(pid);
    portClosed = endpoint
      ? !(await isPortOpen(endpoint.hostname, endpoint.port, 250).catch(() => true))
      : false;
  }
  return { alive, portClosed };
}

function isProcessAlive(pid, killProcess = (targetPid, signal) => process.kill(targetPid, signal)) {
  try {
    killProcess(pid, 0);
    return true;
  } catch (error) {
    return error?.code === 'EPERM';
  }
}

function parseCdpEndpoint(value) {
  let parsed;
  try {
    parsed = new URL(value);
  } catch {
    throw new Error(`invalid Chrome CDP URL: ${value}`);
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    throw new Error(`Chrome CDP URL must use http or https: ${value}`);
  }
  const port = Number(parsed.port || (parsed.protocol === 'https:' ? 443 : 80));
  if (!Number.isInteger(port) || port <= 0 || port > 65535) {
    throw new Error(`Chrome CDP URL has an invalid port: ${value}`);
  }
  return {
    protocol: parsed.protocol,
    origin: parsed.origin,
    hostname: parsed.hostname,
    port,
  };
}

function planCdpEndpointRecovery(endpoint, probe, managedProbes = null) {
  if (probe.status === 'cdp' || !isLegacyLocalCdpEndpoint(endpoint)) {
    return { endpoint, probe, recovery: null, fatal: null };
  }
  const managedProbeResults = normalizedManagedProbeResults(managedProbes);
  const checked = [];
  const blocked = [];
  for (const candidate of managedProbeResults) {
    checked.push(candidate.endpoint.origin);
    if (candidate.probe.status === 'cdp' || candidate.probe.status === 'closed') {
      return {
        endpoint: candidate.endpoint,
        probe: candidate.probe,
        recovery: {
          requested_cdp_url: endpoint.origin,
          fallback_cdp_url: candidate.endpoint.origin,
          selected_cdp_url: candidate.endpoint.origin,
          reason: probe.reason || probe.status,
          checked_cdp_urls: checked,
          blocked_cdp_urls: blocked.map((blockedCandidate) => blockedCandidate.endpoint.origin),
        },
        fatal: null,
      };
    }
    blocked.push(candidate);
  }
  const firstBlocked = blocked[0] ?? managedProbeResults[0] ?? { endpoint: managedCdpEndpoint(), probe: { reason: 'not probed' } };
  return {
    endpoint: null,
    probe: null,
    recovery: {
      requested_cdp_url: endpoint.origin,
      fallback_cdp_url: '',
      selected_cdp_url: '',
      reason: probe.reason || probe.status,
      checked_cdp_urls: checked,
      blocked_cdp_urls: blocked.map((blockedCandidate) => blockedCandidate.endpoint.origin),
    },
    fatal: {
      requested_cdp_url: endpoint.origin,
      checked_cdp_urls: checked,
      checked_endpoint: firstBlocked.endpoint.origin,
      checked_port: firstBlocked.endpoint.port,
      next_action: lsofCommandForPort(firstBlocked.endpoint.port),
      reason: firstBlocked.probe.reason || 'managed Chrome CDP candidate is not usable',
    },
  };
}

function managedCdpEndpoint() {
  return parseCdpEndpoint(`http://${DEFAULT_CDP_HOST}:${DEFAULT_CDP_PORT}`);
}

function managedCdpEndpoints() {
  const endpoints = [];
  for (let port = DEFAULT_CDP_PORT; port <= MANAGED_CDP_MAX_PORT; port += 1) {
    endpoints.push(parseCdpEndpoint(`http://${DEFAULT_CDP_HOST}:${port}`));
  }
  return endpoints;
}

async function probeManagedCdpCandidates(timeoutMs) {
  const results = [];
  for (const endpoint of managedCdpEndpoints()) {
    const probe = await probeCdpEndpoint(endpoint, timeoutMs);
    results.push({ endpoint, probe });
    if (probe.status === 'cdp' || probe.status === 'closed') {
      break;
    }
  }
  return results;
}

function normalizedManagedProbeResults(managedProbes) {
  if (Array.isArray(managedProbes) && managedProbes.length > 0) {
    return managedProbes;
  }
  return [{
    endpoint: managedCdpEndpoint(),
    probe: {
      status: 'closed',
      reason: 'managed Chrome default port selected',
    },
  }];
}

function needsManagedCdpRecovery(endpoint, probe) {
  return probe.status !== 'cdp' && isLegacyLocalCdpEndpoint(endpoint);
}

function cdpRecoveryLogFields(recovery) {
  return {
    requested_cdp_url: recovery.requested_cdp_url,
    fallback_cdp_url: recovery.fallback_cdp_url,
    selected_cdp_url: recovery.selected_cdp_url,
    reason: recovery.reason,
    checked_cdp_urls: recovery.checked_cdp_urls.join(','),
    blocked_cdp_urls: recovery.blocked_cdp_urls.join(','),
  };
}

function cdpRecoveryError(fatal) {
  return new Error([
    `Cannot recover from local Chrome CDP port 922 at ${fatal.requested_cdp_url}: every managed Chrome CDP candidate is occupied by a non-CDP listener.`,
    `Checked endpoint: ${fatal.checked_endpoint}/json/version`,
    `Checked port: ${fatal.checked_port}`,
    `Next action: ${fatal.next_action}`,
  ].join('\n'));
}

function lsofCommandForPort(port) {
  return `lsof -nP -iTCP:${port} -sTCP:LISTEN`;
}

function isLegacyLocalCdpEndpoint(endpoint) {
  return isLocalCdpHost(endpoint.hostname) && endpoint.port === LEGACY_LOCAL_CDP_PORT;
}

function isLocalCdpHost(hostname) {
  return hostname === '127.0.0.1' || hostname === 'localhost' || hostname === '::1' || hostname === '[::1]';
}

async function probeCdpEndpoint(endpoint, timeoutMs) {
  const portOpen = await isPortOpen(endpoint.hostname, endpoint.port, timeoutMs);
  if (!portOpen) {
    return {
      status: 'closed',
      reason: `port ${endpoint.hostname}:${endpoint.port} is closed or unreachable`,
    };
  }
  try {
    await fetchCdpVersion(endpoint, timeoutMs);
    return {
      status: 'cdp',
      reason: 'Chrome CDP version endpoint responded',
    };
  } catch (error) {
    return {
      status: 'open-non-cdp',
      reason: error?.message || String(error),
    };
  }
}

async function waitForCdpVersion(endpoint, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let lastError = null;
  while (Date.now() < deadline) {
    try {
      return await fetchCdpVersion(endpoint, 1200);
    } catch (error) {
      lastError = error;
      await sleep(350);
    }
  }
  const reason = lastError ? ` Last error: ${lastError.message}` : '';
  throw new Error(`Could not reach Chrome CDP at ${endpoint.origin}/json/version within ${timeoutMs}ms.${reason}`);
}

function fetchCdpVersion(endpoint, timeoutMs) {
  return fetchJson(`${endpoint.origin}/json/version`, timeoutMs).then((payload) => {
    if (!payload?.Browser) {
      throw new Error(`Chrome CDP version response did not contain Browser at ${endpoint.origin}/json/version`);
    }
    return payload;
  });
}

function fetchJson(url, timeoutMs) {
  return new Promise((resolvePromise, reject) => {
    const request = http.get(url, { timeout: timeoutMs }, (response) => {
      let body = '';
      response.setEncoding('utf8');
      response.on('data', (chunk) => {
        body += chunk;
      });
      response.on('end', () => {
        try {
          resolvePromise(JSON.parse(body));
        } catch (error) {
          reject(error);
        }
      });
    });
    request.on('timeout', () => {
      request.destroy(new Error(`Timed out fetching ${url}`));
    });
    request.on('error', reject);
  });
}

function isPortOpen(host, port, timeoutMs = 750) {
  return new Promise((resolvePromise) => {
    const socket = new net.Socket();
    let settled = false;
    const finalize = (open) => {
      if (settled) return;
      settled = true;
      socket.destroy();
      resolvePromise(open);
    };
    socket.setTimeout(timeoutMs);
    socket.once('connect', () => finalize(true));
    socket.once('timeout', () => finalize(false));
    socket.once('error', () => finalize(false));
    socket.connect(port, host);
  });
}

function resolveChromeExecutable(explicitPath) {
  if (explicitPath) {
    if (!existsSync(explicitPath)) {
      throw new Error(`Chrome executable was specified but not found: ${explicitPath}`);
    }
    return explicitPath;
  }

  for (const candidate of chromeExecutableCandidates()) {
    if (existsSync(candidate)) {
      return candidate;
    }
  }

  throw new Error('Could not find Google Chrome or Chromium. Install Chrome/Chromium or set JAILGUN_CHROME_EXECUTABLE to the full executable path.');
}

function chromeExecutableCandidates() {
  const home = homedir();
  const candidates = [
    '/usr/bin/google-chrome',
    '/usr/bin/google-chrome-stable',
    '/opt/google/chrome/google-chrome',
    '/usr/bin/chromium',
    '/usr/bin/chromium-browser',
    '/snap/bin/chromium',
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    join(home, 'Applications/Google Chrome.app/Contents/MacOS/Google Chrome'),
    '/Applications/Google Chrome Beta.app/Contents/MacOS/Google Chrome Beta',
    join(home, 'Applications/Google Chrome Beta.app/Contents/MacOS/Google Chrome Beta'),
  ];
  try {
    const result = spawnSync('mdfind', ['kMDItemCFBundleIdentifier == "com.google.Chrome"'], {
      encoding: 'utf8',
      timeout: 2500,
    });
    if (!result.error && result.stdout) {
      for (const appPath of result.stdout.split(/\r?\n/).map((line) => line.trim()).filter(Boolean)) {
        candidates.push(join(appPath, 'Contents/MacOS/Google Chrome'));
      }
    }
  } catch {
    // Spotlight lookup is a convenience only.
  }
  return Array.from(new Set(candidates));
}

function detectProfileLockArtifacts(profileDir) {
  return ['SingletonLock', 'SingletonCookie', 'SingletonSocket', 'Lockfile']
    .map((name) => join(profileDir, name))
    .filter((candidate) => existsSync(candidate));
}

async function clearProfileLockArtifacts(profileDir) {
  for (const candidate of detectProfileLockArtifacts(profileDir)) {
    await rm(candidate, { force: true }).catch(() => undefined);
  }
}

function buildManagedChromeLaunchArgs(options, endpoint) {
  return [
    `--user-data-dir=${options.profileDir}`,
    '--profile-directory=Default',
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-session-crashed-bubble',
    `--remote-debugging-port=${endpoint.port}`,
    `--remote-debugging-address=${endpoint.hostname}`,
    '--new-window',
    'about:blank',
  ];
}

function managedDisplayPlan(env = process.env) {
  const display = String(env.DISPLAY || '').trim();
  if (display) {
    return { display, needsXvfb: false };
  }
  return { display: '', needsXvfb: true };
}

async function startManagedDisplayIfNeeded(env = process.env) {
  const plan = managedDisplayPlan(env);
  if (!plan.needsXvfb) {
    return { display: plan.display, pid: null };
  }
  const displayNumber = chooseXvfbDisplayNumber();
  return startManagedDisplay(displayNumber);
}

function chooseXvfbDisplayNumber() {
  for (let number = 99; number < 150; number += 1) {
    const socketPath = join('/tmp/.X11-unix', `X${number}`);
    const lockPath = `/tmp/.X${number}-lock`;
    if (!existsSync(socketPath) && !existsSync(lockPath)) {
      return number;
    }
  }
  throw new Error('could not find a free Xvfb display number');
}

async function startManagedDisplay(displayNumber) {
  const display = `:${displayNumber}`;
  const executable = resolveXvfbExecutable();
  const child = spawn(executable, [display, '-screen', '0', '1280x720x24', '-nolisten', 'tcp'], {
    detached: true,
    stdio: 'ignore',
    env: { ...process.env },
  });
  child.unref();
  await waitForXvfbDisplay(displayNumber, 5000);
  return { display, pid: child.pid };
}

function resolveXvfbExecutable() {
  const result = spawnSync('which', ['Xvfb'], {
    encoding: 'utf8',
    timeout: 2500,
  });
  if (!result.error && result.status === 0 && result.stdout.trim()) {
    return result.stdout.trim();
  }
  for (const candidate of ['/usr/bin/Xvfb', '/usr/local/bin/Xvfb']) {
    if (existsSync(candidate)) {
      return candidate;
    }
  }
  throw new Error('Could not find Xvfb. Install Xvfb or set DISPLAY before launching managed Chrome.');
}

async function stopManagedDisplay(pid) {
  if (!Number.isInteger(pid) || pid <= 0) {
    return;
  }
  try {
    process.kill(pid, 'SIGTERM');
  } catch (error) {
    if (error?.code !== 'ESRCH') {
      throw error;
    }
    return;
  }
  await waitForManagedPidToExit(pid, 2000).catch(() => undefined);
}

async function waitForXvfbDisplay(displayNumber, timeoutMs) {
  const socketPath = join('/tmp/.X11-unix', `X${displayNumber}`);
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (existsSync(socketPath)) {
      return;
    }
    await sleep(Math.min(100, Math.max(10, deadline - Date.now())));
  }
  throw new Error(`Xvfb did not create ${socketPath} within ${timeoutMs}ms`);
}

async function waitForManagedPidToExit(pid, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (!isProcessAlive(pid)) {
      return;
    }
    await sleep(Math.min(100, Math.max(10, deadline - Date.now())));
  }
}

async function writeManagedBrowserState(stateDir, state) {
  await mkdir(stateDir, { recursive: true });
  await writeFile(join(stateDir, 'managed-browser.pid'), `${state.pid ?? ''}\n`);
  await writeFile(join(stateDir, 'managed-browser.json'), JSON.stringify({
    status: 'running',
    ...state,
  }, null, 2));
}

async function writeManagedBrowserStoppedState(stateDir, endpoint, termination) {
  await mkdir(stateDir, { recursive: true });
  await writeFile(join(stateDir, 'managed-browser.pid'), '\n');
  await writeFile(join(stateDir, 'managed-browser.json'), JSON.stringify({
    status: 'stopped',
    pid: null,
    previousPid: endpoint.pid ?? termination.pid ?? null,
    host: endpoint.cdpUrl ? parseCdpEndpoint(endpoint.cdpUrl).hostname : '',
    port: endpoint.cdpUrl ? parseCdpEndpoint(endpoint.cdpUrl).port : null,
    profileDir: endpoint.profileDir ?? '',
    profileName: endpoint.profileName ?? '',
    stateDir: endpoint.stateDir ?? stateDir,
    cdpUrl: endpoint.cdpUrl ?? '',
    display: endpoint.display ?? '',
    xvfbPid: endpoint.xvfbPid ?? null,
    stoppedAt: timestamp(),
    termination,
  }, null, 2));
}

async function writeManagedBrowserPoolState(stateDir, profiles) {
  await mkdir(stateDir, { recursive: true });
  await writeFile(join(stateDir, 'managed-browsers.json'), JSON.stringify({
    updatedAt: timestamp(),
    profileCount: profiles.length,
    profiles,
  }, null, 2));
}

async function createSourceArchive(options) {
  validateArchiveOptions(options);
  if (options.localArchivePath) {
    return localSourceArchive(options.localArchivePath);
  }
  const tmpParent = options.tmpParent ?? tmpdir();
  await mkdir(tmpParent, { recursive: true });
  const tempRoot = await mkdtemp(join(tmpParent, 'jailgun-source-'));
  const archivePath = join(tempRoot, basename(options.archiveFilename));
  let repoDir = null;
  let cleanupRepo = false;
  try {
    const local = await localRepoPath(options.repoUrl);
    if (local && !options.freshSourceClone) {
      repoDir = local;
    } else {
      repoDir = join(tempRoot, 'repo');
      cleanupRepo = true;
      await runGit(['clone', '--no-local', '--depth=1', local ?? options.repoUrl, repoDir]);
      if (options.refName && options.refName !== 'HEAD') {
        await runGit(['fetch', '--depth=1', 'origin', options.refName], repoDir);
      }
    }
    const ref = options.refName || 'HEAD';
    const commit = (await runGit(['rev-parse', ref], repoDir)).trim();
    const paths = options.mode === 'full' ? null : await listAiSourcePaths(repoDir, ref);
    await gitArchive(repoDir, ref, options.prefix, archivePath, paths);
    const archiveStat = await stat(archivePath);
    if (!archiveStat.isFile() || archiveStat.size === 0) {
      throw new Error(`archive was not created: ${archivePath}`);
    }
    return {
      tempRoot,
      cloneDir: cleanupRepo ? repoDir : '',
      freshSourceClone: cleanupRepo,
      archivePath,
      archiveFilename: basename(archivePath),
      commit,
    };
  } catch (error) {
    await rm(tempRoot, { recursive: true, force: true }).catch(() => undefined);
    throw error;
  }
}

async function localSourceArchive(localArchivePath) {
  const archivePath = resolvePath(localArchivePath);
  const archiveStat = await stat(archivePath);
  if (!archiveStat.isFile() || archiveStat.size === 0) {
    throw new Error(`local archive was not a non-empty file: ${archivePath}`);
  }
  if (!basename(archivePath).endsWith('.tar.gz')) {
    throw new Error('local_archive_path must point to a .tar.gz file');
  }
  return {
    tempRoot: '',
    cloneDir: '',
    freshSourceClone: false,
    archivePath,
    archiveFilename: basename(archivePath),
    commit: 'local-archive',
  };
}

async function localRepoPath(value) {
  if (value.startsWith('file://')) {
    return fileURLToPath(value);
  }
  try {
    const fileStat = await stat(value);
    if (fileStat.isDirectory()) {
      return resolve(value);
    }
  } catch {
    return null;
  }
  return null;
}

function validateArchiveOptions(options) {
  if (!options.localArchivePath && !options.repoUrl?.trim()) {
    throw new Error('repoUrl is required');
  }
  if (options.tmpParent && !isAbsolute(options.tmpParent)) {
    throw new Error('tmpParent must be an absolute path');
  }
  if (!options.prefix.endsWith('/') || options.prefix.startsWith('/') || options.prefix.includes('..')) {
    throw new Error('prefix must be a relative directory ending with /');
  }
  if (!options.archiveFilename.endsWith('.tar.gz')) {
    throw new Error('archiveFilename must end with .tar.gz');
  }
  if (basename(options.archiveFilename) !== options.archiveFilename || options.archiveFilename.includes('..')) {
    throw new Error('archiveFilename must be a safe basename');
  }
  if (options.mode && options.mode !== 'ai-source' && options.mode !== 'full') {
    throw new Error('source archive mode must be ai-source or full');
  }
}

async function listAiSourcePaths(repoDir, ref) {
  const output = await runGit(['ls-tree', '-r', '--name-only', '-z', ref], repoDir);
  const paths = output.split('\0').filter(Boolean).filter(isAiSourcePath);
  if (paths.length === 0) {
    throw new Error('source archive filter produced no useful code or Markdown files');
  }
  return paths;
}

function isAiSourcePath(path) {
  const parts = path.split('/').filter(Boolean);
  if (parts.length === 0) return false;
  if (parts.some((part) => EXCLUDED_DIRECTORIES.has(part.toLowerCase()))) return false;
  const filename = parts[parts.length - 1];
  const lower = filename.toLowerCase();
  if (EXCLUDED_FILENAMES.has(lower)) return false;
  const extension = extname(lower);
  return MARKDOWN_EXTENSIONS.has(extension) || CODE_EXTENSIONS.has(extension) || CODE_FILENAMES.has(lower);
}

async function gitArchive(repoDir, ref, prefix, archivePath, selectedPaths) {
  await new Promise((resolvePromise, reject) => {
    const args = ['archive', '--format=tar.gz', `--prefix=${prefix}`, ref];
    if (selectedPaths) {
      args.push('--', ...selectedPaths);
    }
    const child = spawn('git', args, {
      cwd: repoDir,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    const output = createWriteStream(archivePath);
    let stderr = '';
    let childClosed = false;
    let outputClosed = false;
    let childCode = null;
    let settled = false;
    const fail = (error) => {
      if (settled) return;
      settled = true;
      child.kill();
      output.destroy();
      reject(error);
    };
    const maybeResolve = () => {
      if (settled || !childClosed || !outputClosed) return;
      settled = true;
      if (childCode === 0) {
        resolvePromise();
      } else {
        reject(new Error(`git archive exited ${childCode}: ${stderr.trim()}`));
      }
    };
    child.stderr.setEncoding('utf8');
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
    });
    child.once('error', fail);
    child.once('close', (code) => {
      childClosed = true;
      childCode = code;
      maybeResolve();
    });
    output.once('error', fail);
    output.once('close', () => {
      outputClosed = true;
      maybeResolve();
    });
    child.stdout.pipe(output);
  });
}

async function runGit(args, cwd) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn('git', args, {
      cwd,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    child.stdout.setEncoding('utf8');
    child.stderr.setEncoding('utf8');
    child.stdout.on('data', (chunk) => {
      stdout += chunk;
    });
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
    });
    child.once('error', reject);
    child.once('close', (code) => {
      if (code === 0) {
        resolvePromise(stdout);
      } else {
        reject(new Error(`git ${args.join(' ')} exited ${code}: ${stderr.trim()}`));
      }
    });
  });
}

async function uploadFileToChat(page, archivePath, timeoutMs) {
  const input = page.locator('input[type="file"]').first();
  const inputCount = await input.count().catch(() => 0);
  if (inputCount > 0) {
    await input.setInputFiles(archivePath);
    return;
  }
  const chooserPromise = page.waitForEvent('filechooser', { timeout: timeoutMs });
  const attach = await firstAvailableLocator(page, [
    'button[aria-label*="Attach"]',
    'button[aria-label*="Upload"]',
    'button[title*="Attach"]',
    'button[title*="Upload"]',
    '[data-testid*="attach"]',
    '[data-testid*="upload"]',
    'button:has-text("Attach")',
    'button:has-text("Upload")',
    '[role="button"]:has-text("Attach")',
    '[role="button"]:has-text("Upload")',
  ]);
  try {
    await attach.click({ timeout: timeoutMs });
  } catch (error) {
    void chooserPromise.catch(() => undefined);
    throw error;
  }
  const chooser = await chooserPromise;
  await chooser.setFiles(archivePath);
}

async function confirmUpload(page, archiveFilename, extraSelectors, timeoutMs) {
  const filename = basename(archiveFilename);
  const selectors = [
    ...extraSelectors,
    '[data-testid*="upload-chip"]',
    '[data-testid*="attachment"]',
    `text=${filename}`,
    `[aria-label*="${cssAttr(filename)}"]`,
    `[aria-label*="Attached"]`,
    `[aria-label*="Uploading"]`,
    `[title*="${cssAttr(filename)}"]`,
    'text=Attached',
    'text=Uploading',
  ];
  for (const selector of selectors) {
    try {
      await page.waitForSelector(selector, { timeout: Math.min(timeoutMs, 10000) });
      return true;
    } catch (error) {
      void error;
    }
  }
  return false;
}

async function submitPromptToChat(page, prompt, timeoutMs, hooks = {}) {
  const startedAt = Date.now();
  hooks.log?.('prompt-submit-wait', 'started', 'locating composer for prompt submission', {
    prompt_bytes: String(Buffer.byteLength(prompt, 'utf8')),
  });
  const composer = await waitForChatComposer(page, timeoutMs, hooks, startedAt);

  await composer.fill(prompt, { timeout: timeoutMs });
  await assertComposerHasPrompt(composer, prompt, null);
  hooks.log?.('prompt-submit-wait', 'typed', 'prompt text inserted and verified in composer', {
    elapsed_ms: String(Date.now() - startedAt),
  });

  const deadline = startedAt + timeoutMs;
  let lastObserved = null;
  let lastSummary = '';
  while (Date.now() <= deadline) {
    await hooks.dismiss?.('prompt-submit-readiness');
    await assertComposerHasPrompt(composer, prompt, lastObserved);
    const candidate = await firstVisibleSendCandidate(page, startedAt);
    lastObserved = candidate.observation;
    const summary = sendObservationSummary(lastObserved);
    if (summary !== lastSummary) {
      lastSummary = summary;
      hooks.log?.('prompt-submit-wait', lastObserved.enabled ? 'ready' : 'waiting', lastObserved.enabled ? 'send button is enabled' : 'waiting for send button readiness', stringifyObservationFields(lastObserved));
    }
    if (candidate.button && lastObserved.enabled) {
      await assertComposerHasPrompt(composer, prompt, lastObserved);
      hooks.log?.('prompt-submit-clicked', 'clicking', 'clicking enabled send button', stringifyObservationFields(lastObserved));
      await candidate.button.click({ timeout: Math.max(1, deadline - Date.now()) });
      const accepted = await waitForPromptAcceptance(page, composer, prompt, Math.min(15000, Math.max(1000, deadline - Date.now())), startedAt);
      if (!accepted.accepted) {
        throw new Error(`prompt submit click was not accepted before timeout; last observed state: ${JSON.stringify({ ...lastObserved, acceptance: accepted })}`);
      }
      hooks.log?.('prompt-submit-accepted', 'accepted', 'ChatGPT accepted the prompt submit action', {
        ...stringifyObservationFields(lastObserved),
        acceptance_reason: accepted.reason,
        composer_length: String(accepted.composerLength),
        elapsed_ms: String(Date.now() - startedAt),
      });
      return { acceptanceReason: accepted.reason };
    }
    await sleep(Math.min(250, Math.max(1, deadline - Date.now())));
  }
  throw new Error(`send button did not become enabled before timeout; last observed state: ${JSON.stringify(lastObserved)}`);
}

async function waitForChatComposer(page, timeoutMs, hooks = {}, startedAt = Date.now()) {
  const deadline = startedAt + Math.max(1000, timeoutMs);
  let lastState = null;
  let lastLoggedState = '';
  while (Date.now() <= deadline) {
    await hooks.dismiss?.('prompt-composer-wait');
    const composer = await firstVisibleLocatorOrNull(page, CHAT_COMPOSER_SELECTORS);
    if (composer) {
      hooks.log?.('prompt-submit-wait', 'composer-ready', 'composer is visible', {
        elapsed_ms: String(Date.now() - startedAt),
      });
      return composer;
    }
    lastState = await detectChatAuthState(page);
    if (lastState.state !== lastLoggedState) {
      lastLoggedState = lastState.state;
      hooks.log?.('prompt-submit-wait', 'waiting', 'waiting for ChatGPT composer', {
        auth_state: lastState.state,
        elapsed_ms: String(Date.now() - startedAt),
      }, lastState.state === 'unknown' ? 'info' : 'warn');
    }
    if (composerAuthStateIsActionable(lastState)) {
      hooks.authState?.(lastState);
      throw composerAuthError(lastState);
    }
    await sleep(Math.min(500, Math.max(1, deadline - Date.now())));
  }

  lastState = await detectChatAuthState(page);
  hooks.authState?.(lastState);
  if (composerAuthStateIsActionable(lastState)) {
    throw composerAuthError(lastState);
  }
  throw new Error(`ChatGPT composer did not appear before timeout; auth_state=${lastState.state}; reason=${lastState.reason || 'unknown'}`);
}

function composerAuthStateIsActionable(state) {
  return ['auth-required', 'code-requested', 'manual-browser-required', 'session-expired'].includes(state?.state);
}

function composerAuthError(state) {
  if (state.state === 'manual-browser-required') {
    return manualBrowserRequired(`manual-browser-required: ${state.reason || 'manual browser action is required before prompt submission'}`);
  }
  const error = new Error(`${state.state}: ${state.reason || composerAuthReason(state.state)}`);
  error.authState = state.state;
  return error;
}

function composerAuthReason(state) {
  switch (state) {
    case 'auth-required':
      return 'ChatGPT login is required before prompt submission';
    case 'code-requested':
      return 'ChatGPT verification code is required before prompt submission';
    case 'session-expired':
      return 'ChatGPT session expired before prompt submission';
    default:
      return 'ChatGPT composer was not available';
  }
}

async function detectChatAuthState(page) {
  const pageUrl = page.url();
  const composerDetected = await hasChatComposer(page);
  if (composerDetected) {
    return {
      state: 'ready',
      pageUrl,
      composerDetected: true,
      codeRequested: false,
      reason: null,
      manualAction: null,
    };
  }

  const bodyText = await page.locator('body').innerText({ timeout: 2500 }).catch(() => '');
  const normalized = bodyText.replace(/\s+/g, ' ').trim();
  const lower = normalized.toLowerCase();
  const manualAction = manualAuthActionFromText(lower);
  if (manualAction) {
    return {
      state: manualAction.action === 'session-expired' ? 'session-expired' : 'manual-browser-required',
      pageUrl,
      composerDetected: false,
      codeRequested: false,
      reason: manualAction.reason,
      manualAction,
    };
  }
  const codeRequested = await hasVisibleCodeInput(page);
  if (codeRequested || /\b(code|verification code|one-time|one time)\b/i.test(normalized)) {
    return {
      state: 'code-requested',
      pageUrl,
      composerDetected: false,
      codeRequested: true,
      reason: null,
      manualAction: null,
    };
  }
  if (await hasLoginControl(page)) {
    return {
      state: 'auth-required',
      pageUrl,
      composerDetected: false,
      codeRequested: false,
      reason: null,
      manualAction: null,
    };
  }
  return {
    state: 'unknown',
    pageUrl,
    composerDetected: false,
    codeRequested: false,
    reason: compact(normalized, 180) || 'ChatGPT composer was not detected',
    manualAction: null,
  };
}

async function hasChatComposer(page) {
  for (const selector of CHAT_COMPOSER_SELECTORS) {
    const locator = page.locator(selector).first();
    if (await locator.count().catch(() => 0) > 0 && await locator.isVisible().catch(() => false)) {
      return true;
    }
  }
  return false;
}

async function hasLoginControl(page) {
  for (const selector of [
    'input[type="email"]',
    'input[name*="email" i]',
    'button:has-text("Log in")',
    'button:has-text("Sign in")',
    'a:has-text("Log in")',
    'a:has-text("Sign in")',
  ]) {
    const locator = page.locator(selector).first();
    if (await locator.count().catch(() => 0) > 0 && await locator.isVisible().catch(() => false)) {
      return true;
    }
  }
  return false;
}

async function hasVisibleCodeInput(page) {
  for (const selector of [
    'input[autocomplete="one-time-code"]',
    'input[name*="code" i]',
    'input[inputmode="numeric"]',
    'input[type="tel"]',
  ]) {
    const locator = page.locator(selector).first();
    if (await locator.count().catch(() => 0) > 0 && await locator.isVisible().catch(() => false)) {
      return true;
    }
  }
  return false;
}

function manualAuthActionFromText(lowerText) {
  if (/\b(password|enter your password)\b/.test(lowerText)) {
    return { action: 'manual-browser-required', reason: 'password prompt detected' };
  }
  if (/\b(captcha|recaptcha|hcaptcha|verify you are human)\b/.test(lowerText)) {
    return { action: 'manual-browser-required', reason: 'captcha prompt detected' };
  }
  if (/\b(passkey|security key|hardware key|authenticator app)\b/.test(lowerText)) {
    return { action: 'manual-browser-required', reason: 'passkey or security-key prompt detected' };
  }
  if (/\b(sms|text message|phone|call your phone|whatsapp)\b/.test(lowerText) && !/\bemail\b/.test(lowerText)) {
    return { action: 'manual-browser-required', reason: 'phone or SMS verification prompt detected' };
  }
  if (/\b(session expired|log in again|sign in again)\b/.test(lowerText)) {
    return { action: 'session-expired', reason: 'session expired prompt detected' };
  }
  return null;
}

async function fillKnownEmailIfPresent(page, emailHint) {
  const email = String(emailHint || '').trim();
  if (!email) {
    return false;
  }
  const input = await firstVisibleLocatorOrNull(page, [
    'input[type="email"]',
    'input[name*="email" i]',
    'input[autocomplete="email"]',
  ]);
  if (!input) {
    const login = await firstVisibleLocatorOrNull(page, [
      'button:has-text("Log in")',
      'button:has-text("Sign in")',
      'a:has-text("Log in")',
      'a:has-text("Sign in")',
    ]);
    if (login) {
      await login.click({ timeout: 10000 });
      await page.waitForLoadState('domcontentloaded', { timeout: 10000 }).catch(() => undefined);
      return fillKnownEmailIfPresent(page, email);
    }
    return false;
  }
  await input.fill(email, { timeout: 10000 });
  await input.press('Enter', { timeout: 10000 }).catch(() => undefined);
  await page.waitForLoadState('domcontentloaded', { timeout: 10000 }).catch(() => undefined);
  return true;
}

async function selectEmailCodeControl(page) {
  const candidates = await authControlCandidates(page);
  const emailCandidates = candidates.filter((candidate) => {
    const text = candidate.text.toLowerCase();
    return candidate.visible
      && /\bemail\b/.test(text)
      && /\b(code|verification|verify|send|continue|one-time|one time)\b/.test(text)
      && !/\b(sms|text|phone|call|passkey|security key|authenticator|whatsapp)\b/.test(text);
  });
  if (emailCandidates.length === 0) {
    return { clicked: false, reason: 'no obvious email-code control was visible' };
  }
  if (emailCandidates.length > 1) {
    return { clicked: false, reason: 'multiple possible email-code controls were visible' };
  }
  const candidate = emailCandidates[0];
  await page.locator(AUTH_CONTROL_SELECTOR).nth(candidate.index).click({ timeout: 10000 });
  await page.waitForLoadState('domcontentloaded', { timeout: 10000 }).catch(() => undefined);
  return {
    clicked: true,
    destinationHint: compact(candidate.text, 120),
  };
}

async function authControlCandidates(page) {
  return page.locator(AUTH_CONTROL_SELECTOR).evaluateAll((elements) => elements.map((el, index) => {
    const rect = el.getBoundingClientRect();
    const style = window.getComputedStyle(el);
    const text = [
      el.getAttribute('aria-label'),
      el.getAttribute('title'),
      el.textContent,
      el.getAttribute('value'),
    ].filter(Boolean).join(' ');
    return {
      index,
      text: String(text || '').replace(/\s+/g, ' ').trim(),
      visible: rect.width > 0 && rect.height > 0 && style.visibility !== 'hidden' && style.display !== 'none',
    };
  })).catch(() => []);
}

async function submitVerificationCode(page, code) {
  const compactCode = String(code).replace(/\s+/g, '');
  const singleInputs = await visibleCodeInputs(page);
  if (singleInputs >= compactCode.length && compactCode.length > 1) {
    for (let index = 0; index < compactCode.length; index += 1) {
      await page.locator(CODE_INPUT_SELECTOR).nth(index).fill(compactCode[index], { timeout: 5000 });
    }
  } else {
    const input = await firstVisibleLocatorOrNull(page, [
      'input[autocomplete="one-time-code"]',
      'input[name*="code" i]',
      'input[inputmode="numeric"]',
      'input[type="tel"]',
    ]);
    if (!input) {
      throw new Error('verification code input was not visible');
    }
    await input.fill(compactCode, { timeout: 10000 });
  }
  const submit = await firstVisibleLocatorOrNull(page, [
    'button:has-text("Continue")',
    'button:has-text("Verify")',
    'button:has-text("Submit")',
    'button:has-text("Next")',
    'input[type="submit"]',
  ]);
  if (submit) {
    await submit.click({ timeout: 10000 });
    await page.waitForLoadState('domcontentloaded', { timeout: 10000 }).catch(() => undefined);
  }
}

async function visibleCodeInputs(page) {
  const locator = page.locator(CODE_INPUT_SELECTOR);
  const total = await locator.count().catch(() => 0);
  let visible = 0;
  for (let index = 0; index < total; index += 1) {
    if (await locator.nth(index).isVisible().catch(() => false)) {
      visible += 1;
    }
  }
  return visible;
}

async function waitForAuthReadyOrAction(page, timeoutMs) {
  const deadline = Date.now() + Math.max(1000, timeoutMs);
  let last = await detectChatAuthState(page);
  while (Date.now() <= deadline) {
    last = await detectChatAuthState(page);
    if (last.state === 'ready' || last.manualAction) {
      return last;
    }
    await sleep(500);
  }
  return last;
}

async function firstVisibleLocatorOrNull(page, selectors) {
  for (const selector of selectors) {
    const locator = page.locator(selector).first();
    if (await locator.count().catch(() => 0) > 0 && await locator.isVisible().catch(() => false)) {
      return locator;
    }
  }
  return null;
}

function manualBrowserRequired(reason) {
  const error = new Error(reason || 'manual browser auth is required');
  error.manualBrowserRequired = true;
  return error;
}

function isManualBrowserRequiredError(error) {
  return Boolean(error?.manualBrowserRequired);
}

async function firstAvailableLocator(page, selectors) {
  let firstFound = null;
  for (const selector of selectors) {
    const locator = page.locator(selector).first();
    const count = await locator.count().catch(() => 0);
    if (count > 0) {
      firstFound = firstFound ?? locator;
      const visible = await locator.isVisible().catch(() => false);
      if (visible) {
        return locator;
      }
    }
  }
  if (firstFound) {
    return firstFound;
  }
  throw new Error(`missing chat control: ${selectors.join(',')}`);
}

async function firstVisibleSendCandidate(page, startedAt) {
  let candidateObservation = null;
  for (const selector of SEND_BUTTON_SELECTORS) {
    const locator = page.locator(selector);
    const total = await locator.count().catch(() => 0);
    if (total === 0) {
      candidateObservation = emptySendObservation(selector, startedAt);
      continue;
    }
    for (let index = 0; index < total; index += 1) {
      const button = locator.nth(index);
      const observation = await observeSendButton(button, selector, total, startedAt);
      if (!candidateObservation || observation.visible) {
        candidateObservation = observation;
      }
      if (observation.visible) {
        return { button, observation };
      }
    }
  }
  return { button: null, observation: candidateObservation ?? emptySendObservation(SEND_BUTTON_SELECTORS[0], startedAt) };
}

async function observeSendButton(button, selector, count, startedAt) {
  const visible = await button.isVisible().catch(() => false);
  const ariaDisabled = await button.getAttribute('aria-disabled').catch(() => null);
  const disabledAttr = await button.getAttribute('disabled').catch(() => null);
  const ariaLabel = await button.getAttribute('aria-label').catch(() => null);
  const title = await button.getAttribute('title').catch(() => null);
  const dataState = await button.getAttribute('data-state').catch(() => null);
  const text = await button.textContent().catch(() => null);
  const label = firstNonEmpty([ariaLabel, title, text]);
  const explicitEnabled = await button.isEnabled().catch(() => false);
  const enabled = visible && explicitEnabled && disabledAttr === null && ariaDisabled !== 'true' && dataState !== 'disabled';
  const uploadState = firstMatching([ariaLabel, title, dataState, text], /upload|attach|processing|prepar/i);
  let disabledReason = null;
  if (!visible) {
    disabledReason = 'not-visible';
  } else if (!enabled) {
    disabledReason = uploadState ? `upload-state:${uploadState}` : 'disabled';
  }
  return {
    selector,
    count,
    visible,
    enabled,
    elapsedMs: Date.now() - startedAt,
    disabledReason,
    uploadState,
    ariaDisabled,
    disabledAttr,
    label,
  };
}

function emptySendObservation(selector, startedAt) {
  return {
    selector,
    count: 0,
    visible: false,
    enabled: false,
    elapsedMs: Date.now() - startedAt,
    disabledReason: 'not-found',
    uploadState: null,
    ariaDisabled: null,
    disabledAttr: null,
    label: null,
  };
}

function sendObservationSummary(observation) {
  if (!observation) {
    return 'missing';
  }
  return [
    observation.selector,
    observation.count,
    observation.visible ? 'visible' : 'hidden',
    observation.enabled ? 'enabled' : 'disabled',
    observation.disabledReason || '',
    observation.uploadState || '',
  ].join('|');
}

function stringifyObservationFields(observation) {
  return {
    selector: observation?.selector || '',
    count: String(observation?.count ?? 0),
    visible: String(Boolean(observation?.visible)),
    enabled: String(Boolean(observation?.enabled)),
    elapsed_ms: String(observation?.elapsedMs ?? 0),
    disabled_reason: observation?.disabledReason || '',
    upload_state: observation?.uploadState || '',
    aria_disabled: observation?.ariaDisabled || '',
    disabled_attr: observation?.disabledAttr || '',
    label: observation?.label || '',
  };
}

async function waitForPromptAcceptance(page, composer, prompt, timeoutMs, startedAt) {
  const deadline = Date.now() + Math.max(1000, timeoutMs);
  let lastComposerText = prompt;
  while (Date.now() <= deadline) {
    const [composerText, status] = await Promise.all([
      readComposerText(composer).catch(() => ''),
      readGenerationStatus(page).catch(() => ({ activeStop: false, finalActions: 0 })),
    ]);
    lastComposerText = composerText;
    if (status.activeStop) {
      return {
        accepted: true,
        reason: 'active-stop-visible',
        composerLength: composerText.length,
        elapsedMs: Date.now() - startedAt,
      };
    }
    if (!textLooksInserted(composerText, prompt)) {
      return {
        accepted: true,
        reason: composerText.trim() ? 'composer-changed' : 'composer-cleared',
        composerLength: composerText.length,
        elapsedMs: Date.now() - startedAt,
      };
    }
    await sleep(150);
  }
  return {
    accepted: false,
    reason: 'timeout',
    composerLength: lastComposerText.length,
    elapsedMs: Date.now() - startedAt,
  };
}

async function assertComposerHasPrompt(composer, prompt, lastObserved) {
  const text = await readComposerText(composer);
  if (!textLooksInserted(text, prompt)) {
    throw new Error(`composer text disappeared before send; observed ${text.length} characters; last state: ${JSON.stringify(lastObserved)}`);
  }
}

async function readComposerText(composer) {
  const value = await composer.inputValue({ timeout: 1000 }).catch(() => null);
  if (value !== null) {
    return value;
  }
  return (await composer.textContent({ timeout: 1000 }).catch(() => '')) ?? '';
}

function textLooksInserted(text, expected) {
  const normalize = (value) => String(value || '')
    .replace(/[\u200B-\u200D\uFEFF]/g, '')
    .replace(/\s+/g, ' ')
    .trim();
  const haystack = normalize(text);
  const needle = normalize(expected);
  if (!needle || !haystack) {
    return false;
  }
  if (haystack.includes(needle) || needle.includes(haystack)) {
    return true;
  }
  const compactHaystack = haystack.replace(/\s+/g, '');
  const compactNeedle = needle.replace(/\s+/g, '');
  if (compactHaystack.includes(compactNeedle) || compactNeedle.includes(compactHaystack)) {
    return true;
  }
  let sharedPrefix = 0;
  const max = Math.min(compactHaystack.length, compactNeedle.length);
  while (sharedPrefix < max && compactHaystack[sharedPrefix] === compactNeedle[sharedPrefix]) {
    sharedPrefix += 1;
  }
  return sharedPrefix >= Math.ceil(compactNeedle.length * 0.95);
}

function isTransientNavigationError(error) {
  const message = error?.message || String(error);
  return /Execution context was destroyed|most likely because of a navigation|Cannot find context with specified id/i.test(message);
}

async function discoverTarCandidates(page, targetName = '') {
  if (typeof page?.__jailgunDiscoverTarCandidates === 'function') {
    return page.__jailgunDiscoverTarCandidates(targetName);
  }
  const discovery = await page.evaluate(({ targetName: target }) => {
    const controls = Array.from(document.querySelectorAll('a,button,[role="button"],[download],[href]'));
    const assistantRoots = Array.from(document.querySelectorAll('[data-message-author-role="assistant"]'));
    // Detect A/B feedback response containers
    const abFeedbackActive = /giving feedback on a new version|which response do you prefer/i.test(document.body?.innerText || '');
    let abResponseRoots = [];
    if (abFeedbackActive) {
      const abSelectors = [
        '[data-testid*="response-turn"]',
        '[data-testid*="response-option"]',
        '[class*="response-turn"]',
        '[class*="comparison"]',
      ];
      for (const sel of abSelectors) {
        abResponseRoots = Array.from(document.querySelectorAll(sel));
        if (abResponseRoots.length >= 2) break;
      }
    }
    const textOf = (el) => String(el?.innerText || el?.textContent || '').replace(/\s+/g, ' ').trim();
    const attr = (el, name) => el?.getAttribute?.(name) || '';
    const href = (el) => el?.href || attr(el, 'href');
    const closestAssistant = (el) => el?.closest?.('[data-message-author-role="assistant"]') || null;
    const uploadChip = (el) => el?.closest?.('[data-testid*="upload-chip"]') || null;
    const visible = (el) => {
      const style = window.getComputedStyle(el);
      const rect = el.getBoundingClientRect();
      return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
    };
    const disabled = (el) => el.hasAttribute?.('disabled') || /^true$/i.test(attr(el, 'aria-disabled'));
    const tar = (value) => /\.tar(?:\(\d+\))?\.gz(?:$|[?#\s)])/i.test(String(value || ''));
    const tex = (value) => /\.tex(?:$|[?#\s)])/i.test(String(value || ''));
    const genericArtifactNamePattern = /(?:^|[\s"'`(])([A-Za-z0-9][A-Za-z0-9._-]*\.[A-Za-z0-9][A-Za-z0-9._-]{0,15})(?:$|[?#\s)"'`,])/gi;
    const normalizeComparable = (value) => String(value || '')
      .replace(/\.tar\(\d+\)\.gz/gi, '.tar.gz')
      .replace(/\.tgz/gi, '.tar.gz')
      .replace(/\s+/g, ' ')
      .toLowerCase();
    const artifactNamesFromText = (value) => {
      const names = [];
      const pattern = new RegExp(genericArtifactNamePattern);
      let match;
      while ((match = pattern.exec(String(value || ''))) !== null) {
        if (match[1]) names.push(match[1]);
      }
      return names;
    };
    const artifactNamesFromHref = (value) => {
      try {
        const url = new URL(value, 'https://example.invalid/');
        const name = url.pathname.split('/').filter(Boolean).pop() || '';
        return name.includes('.') ? [name] : [];
      } catch {
        const name = String(value || '').split(/[/?#]/).filter(Boolean).pop() || '';
        return name.includes('.') ? [name] : [];
      }
    };
    const artifactNamesFromValues = (values) => Object.entries(values)
      .flatMap(([name, value]) => name === 'href' ? artifactNamesFromHref(value) : artifactNamesFromText(value));
    const downloadAction = (value) => /\b(download|downloadable|save|export)\b/i.test(String(value || ''));
    const archiveNoun = (value) => /\b(tarball|tar|archive|artifact)\b/i.test(String(value || ''));
    const texNoun = (value) => /\b(tex|latex|chapter|file|artifact)\b/i.test(String(value || ''));
    const fileNoun = (value) => /\b(file|artifact|download|export|save|json|markdown|csv|text|document)\b/i.test(String(value || ''));
    const clickable = (entry) => entry.tag === 'button' || entry.role === 'button' || Boolean(entry.href || entry.download);
    const normalizedTarget = normalizeComparable(String(target || '').trim());
    const targetIsTex = /\.tex$/i.test(normalizedTarget);
    const targetIsTar = /\.tar\.gz$/i.test(normalizedTarget);
    const targetIsGenericFile = normalizedTarget !== '' && !targetIsTar && !targetIsTex;
    const candidates = [];
    for (let index = 0; index < controls.length; index += 1) {
      const el = controls[index];
      const assistant = closestAssistant(el);
      const inABResponse = abResponseRoots.length > 0 && abResponseRoots.some((root) => root.contains(el));
      if ((assistantRoots.length > 0 && !assistant && !inABResponse) || uploadChip(el)) continue;
      if (!visible(el) || disabled(el)) continue;
      const tag = String(el.tagName || '').toLowerCase();
      const role = attr(el, 'role').toLowerCase();
      const text = textOf(el);
      const entry = {
        index,
        text,
        href: href(el),
        download: attr(el, 'download'),
        aria: attr(el, 'aria-label'),
        title: attr(el, 'title'),
        tag,
        role,
        assistantIndex: assistant ? assistantRoots.indexOf(assistant) : null,
        score: 0,
      };
      const haystack = `${entry.text} ${entry.href} ${entry.download} ${entry.aria} ${entry.title}`;
      const explicitTar = tar(haystack);
      const explicitTex = tex(haystack);
      const sourceValueMap = {
        text: entry.text,
        href: entry.href,
        download: entry.download,
        aria: entry.aria,
        title: entry.title,
      };
      const artifactSources = [];
      for (const [name, value] of Object.entries(sourceValueMap)) {
        if (normalizedTarget && normalizeComparable(value).includes(normalizedTarget)) {
          artifactSources.push(name);
        }
      }
      const artifactNamesInControl = artifactNamesFromValues(sourceValueMap).map(normalizeComparable);
      const conflictingArtifactName = Boolean(
        normalizedTarget && artifactNamesInControl.some((name) => name !== normalizedTarget)
      );
      const genericArchiveDownload = Boolean(
        assistant
          && downloadAction(haystack)
          && archiveNoun(haystack)
      );
      const genericTexDownload = Boolean(
        targetIsTex
          && assistant
          && downloadAction(haystack)
          && texNoun(haystack)
      );
      const genericFileDownload = Boolean(
        targetIsGenericFile
          && assistant
          && downloadAction(haystack)
          && fileNoun(haystack)
          && !conflictingArtifactName
          && artifactNamesInControl.length === 0
      );
      const hasCandidateSignal = targetIsGenericFile
        ? artifactSources.length > 0 || genericFileDownload
        : explicitTar || explicitTex || genericArchiveDownload || genericTexDownload;
      if (!clickable(entry) || !hasCandidateSignal) continue;
      entry.label = entry.text || entry.download || entry.href || entry.aria || entry.title;
      entry.fileKind = targetIsGenericFile || genericFileDownload ? 'downloaded-file' : explicitTex ? 'downloaded-tex' : explicitTar || genericArchiveDownload ? 'downloaded-archive' : 'downloaded-file';
      entry.artifactSources = artifactSources;
      entry.score += targetIsGenericFile ? artifactSources.length > 0 ? 260 : 120 : explicitTex ? 260 : explicitTar ? 200 : 120;
      if (/download/i.test(haystack)) entry.score += 100;
      if (tar(entry.download)) entry.score += 90;
      if (tar(entry.href)) entry.score += 80;
      if (tar(entry.text)) entry.score += 60;
      if (tex(entry.download)) entry.score += 120;
      if (tex(entry.href)) entry.score += 100;
      if (tex(entry.text)) entry.score += 80;
      if (genericArchiveDownload) entry.score += 30;
      if (genericTexDownload) entry.score += 80;
      if (genericFileDownload) entry.score += 80;
      if (artifactSources.length > 0) entry.score += 220;
      if (targetIsTex && explicitTex) entry.score += 200;
      if (targetIsTex && explicitTar) entry.score -= 40;
      if (tag === 'button' || role === 'button') entry.score += 20;
      if (tag === 'a') entry.score += 10;
      if (assistant) entry.score += 30;
      candidates.push(entry);
    }
    const roots = assistantRoots.length > 0 ? assistantRoots : [document.body];
    const lastAssistantText = assistantRoots.length > 0 ? textOf(assistantRoots[assistantRoots.length - 1]) : textOf(document.body);
    const lastTextLength = roots.reduce((sum, root) => sum + textOf(root).length, 0);
    return {
      assistantRootCount: assistantRoots.length,
      scannedControlCount: controls.length,
      candidates,
      lastTextLength,
      lastTextPreview: lastAssistantText.slice(0, 240),
      abFeedbackActive,
      abResponseCount: abResponseRoots.length,
    };
  }, { targetName });
  try {
    discovery.artifactConversationLinks = await discoverArtifactConversationLinks(page, targetName, page.url?.() || '');
  } catch (error) {
    discovery.artifactConversationLinks = [];
    discovery.artifactConversationLinksError = error?.message || String(error);
  }
  return discovery;
}

async function discoverArtifactConversationLinks(page, targetName = '', currentUrl = '') {
  if (typeof page?.__jailgunDiscoverArtifactConversationLinks === 'function') {
    return page.__jailgunDiscoverArtifactConversationLinks(targetName, currentUrl);
  }
  return page.evaluate(({ targetName: target, currentUrl: current }) => {
    const selector = 'a[href]';
    const tarNamePattern = /\.tar(?:\(\d+\))?\.gz(?:$|[?#\s)])/i;
    const artifactNamePattern = /(?:^|[/\s"'`(])([A-Za-z0-9][A-Za-z0-9._-]*\.(?:tar(?:\(\d+\))?\.gz|tgz|tex|jsonl?|md|markdown|txt|csv|tsv|ya?ml|toml|pdf|png|jpe?g|webp|gif|svg|zip))(?:$|[?#\s)"'`,])/i;
    const chapterPattern = /\bchapter[\s_-]*0*(\d{1,4})\b/i;
    const normalizeText = (value) => String(value || '').replace(/\s+/g, ' ').trim();
    const normalizeComparable = (value) => normalizeText(value)
      .replace(/\.tar\(\d+\)\.gz/gi, '.tar.gz')
      .replace(/\.tgz/gi, '.tar.gz')
      .replace(/[_-]+/g, ' ')
      .toLowerCase();
    const extractChapter = (value) => {
      const match = String(value || '').match(chapterPattern);
      return match ? String(Number(match[1])) : '';
    };
    const isLocaleSegment = (value) => /^[a-z]{2}(?:-[A-Za-z]{2})?$/.test(value);
    const normalizeConversationUrl = (value, baseValue) => {
      try {
        const url = new URL(value, baseValue || 'https://chatgpt.com/');
        const base = baseValue ? new URL(baseValue, 'https://chatgpt.com/') : null;
        if (url.hostname !== 'chatgpt.com' || (base && base.hostname === 'chatgpt.com' && url.origin !== base.origin)) {
          return null;
        }
        const parts = url.pathname.split('/').filter(Boolean);
        let id = '';
        if (parts[0] === 'c' && parts[1]) {
          id = parts[1];
        } else if (parts.length === 3 && isLocaleSegment(parts[0]) && parts[1] === 'c') {
          id = parts[2];
        }
        if (!id || !/^[A-Za-z0-9-]+$/.test(id)) {
          return null;
        }
        return {
          url: `${url.origin}/c/${id}`,
          conversationId: id,
        };
      } catch {
        return null;
      }
    };
    const artifactSignalsFor = (value) => {
      const signals = [];
      if (artifactNamePattern.test(value)) signals.push('artifact-name');
      if (tarNamePattern.test(value)) signals.push('tar-name');
      if (/\b(?:tarball|tar\.?gz|tar)\b/i.test(value)) signals.push('tar');
      if (/\bartifacts?\b/i.test(value)) signals.push('artifact');
      if (/\barchive\b/i.test(value)) signals.push('archive');
      if (/\blatex\b/i.test(value) && /\b(?:creat(?:e|ed|es|ing|ion)|generat(?:e|ed|es|ing|ion)|build|export|download)\b/i.test(value)) {
        signals.push('latex-creation');
      }
      return signals;
    };
    const baseUrl = current || document.location?.href || 'https://chatgpt.com/';
    const currentConversation = normalizeConversationUrl(baseUrl, baseUrl);
    const targetChapter = extractChapter(target);
    const normalizedTarget = normalizeComparable(target);
    const targetStem = normalizedTarget.replace(/\.tar\.gz$/i, '');
    const byUrl = new Map();
    const anchors = Array.from(document.querySelectorAll(selector));
    anchors.forEach((anchor, index) => {
      if (anchor.closest?.('[data-testid*="upload-chip"]')) {
        return;
      }
      const href = anchor.href || anchor.getAttribute?.('href') || '';
      const normalizedUrl = normalizeConversationUrl(href, baseUrl);
      if (!normalizedUrl || normalizedUrl.url === currentConversation?.url) {
        return;
      }
      const text = normalizeText(anchor.innerText || anchor.textContent || '');
      const aria = normalizeText(anchor.getAttribute?.('aria-label') || '');
      const title = normalizeText(anchor.getAttribute?.('title') || '');
      const haystack = `${text} ${aria} ${title}`;
      if (/open conversation options/i.test(haystack)) {
        return;
      }
      const comparableHaystack = normalizeComparable(haystack);
      const artifactSignals = artifactSignalsFor(haystack);
      if (
        normalizedTarget
          && comparableHaystack.includes(normalizedTarget)
          && !artifactSignals.includes('artifact-name')
      ) {
        artifactSignals.push('artifact-name');
      }
      if (artifactSignals.length === 0) {
        return;
      }
      const linkChapter = extractChapter(haystack);
      const targetMatched = !normalizedTarget
        || (targetChapter ? linkChapter === targetChapter : Boolean(
          normalizedTarget && comparableHaystack.includes(normalizedTarget)
          || targetStem && comparableHaystack.includes(targetStem)
        ));
      if (!targetMatched) {
        return;
      }
      let score = 100 + artifactSignals.length * 25;
      if (linkChapter) score += 20;
      if (targetChapter && linkChapter === targetChapter) score += 120;
      if (normalizedTarget && comparableHaystack.includes(normalizedTarget)) score += 100;
      if (targetStem && comparableHaystack.includes(targetStem)) score += 60;
      if (artifactSignals.includes('artifact-name')) score += 70;
      if (artifactSignals.includes('tar-name')) score += 60;
      if (artifactSignals.includes('artifact')) score += 40;
      if (artifactSignals.includes('latex-creation')) score += 30;
      const candidate = {
        index,
        url: normalizedUrl.url,
        href,
        text,
        aria,
        title,
        score,
        selector,
        tagName: String(anchor.tagName || 'a').toLowerCase(),
        conversationId: normalizedUrl.conversationId,
        chapter: linkChapter,
        targetMatched,
        artifactSignals,
      };
      const existing = byUrl.get(candidate.url);
      if (!existing || candidate.score > existing.score || (candidate.score === existing.score && candidate.index < existing.index)) {
        byUrl.set(candidate.url, candidate);
      }
    });
    return Array.from(byUrl.values()).sort((left, right) => right.score - left.score || left.index - right.index);
  }, { targetName, currentUrl });
}

function rankCandidates(candidates, targetName) {
  const target = normalizeArtifactComparable(targetName);
  const targetStem = target
    .replace(/\.tar\.gz$/i, '')
    .replace(/\.tex$/i, '');
  const targetIsTex = isTexNameLike(targetName);
  const targetIsGenericFile = target !== '' && !isTarGzNameLike(target) && !targetIsTex;
  return [...candidates]
    .filter((candidate) => !isDocumentTarLabelOnlyCandidate(candidate))
    .filter((candidate) => !targetIsGenericFile || !hasConflictingArtifactName(candidate, target))
    .map((candidate) => {
      const kind = candidateFileKind(candidate, targetName);
      let scoreBonus = 0;
      if (targetIsTex && kind === 'downloaded-tex') {
        scoreBonus += 500;
      } else if (targetIsTex && kind === 'downloaded-archive') {
        scoreBonus += 20;
      } else if (targetIsGenericFile && kind === 'downloaded-file') {
        scoreBonus += 300;
      }
      if (target) {
        const haystack = normalizeArtifactComparable(`${candidate.text} ${candidate.href} ${candidate.download} ${candidate.aria} ${candidate.title}`);
        if (haystack.includes(target)) {
          scoreBonus += 600;
        } else if (targetStem && haystack.includes(targetStem)) {
          scoreBonus += 120;
        }
      }
      return scoreBonus === 0 ? candidate : { ...candidate, score: candidate.score + scoreBonus };
    })
    .sort((a, b) => b.score - a.score);
}

function isDocumentTarLabelOnlyCandidate(candidate) {
  const href = String(candidate?.href || '');
  if (candidate?.assistantIndex != null) {
    return false;
  }
  if (tarNameLike(candidate?.download) || tarNameLike(href)) {
    return false;
  }
  return tarNameLike(`${candidate?.text || ''} ${candidate?.aria || ''} ${candidate?.title || ''} ${candidate?.label || ''}`);
}

const ARTIFACT_NAME_RE = /(?:^|[\s"'`(])([A-Za-z0-9][A-Za-z0-9._-]*\.[A-Za-z0-9][A-Za-z0-9._-]{0,15})(?:$|[?#\s)"'`,])/gi;

function hasConflictingArtifactName(candidate, normalizedTarget) {
  if (!normalizedTarget) {
    return false;
  }
  const names = artifactNamesFromCandidate(candidate)
    .map(normalizeArtifactComparable);
  return names.some((name) => name !== normalizedTarget);
}

function artifactNamesFromCandidate(candidate) {
  return [
    ...artifactNamesFromText(`${candidate?.text || ''} ${candidate?.download || ''} ${candidate?.aria || ''} ${candidate?.title || ''} ${candidate?.label || ''}`),
    ...artifactNamesFromHref(candidate?.href || ''),
  ];
}

function artifactNamesFromHref(value) {
  try {
    const url = new URL(value, 'https://example.invalid/');
    const name = url.pathname.split('/').filter(Boolean).pop() || '';
    return name.includes('.') ? [name] : [];
  } catch {
    const name = String(value || '').split(/[/?#]/).filter(Boolean).pop() || '';
    return name.includes('.') ? [name] : [];
  }
}

function artifactNamesFromText(value) {
  const names = [];
  const pattern = new RegExp(ARTIFACT_NAME_RE);
  let match;
  while ((match = pattern.exec(String(value || ''))) !== null) {
    if (match[1]) {
      names.push(match[1]);
    }
  }
  return names;
}

function tarNameLike(value) {
  return /\.tar(?:\(\d+\))?\.gz(?:$|[?#\s)])/i.test(String(value || ''));
}

function texNameLike(value) {
  return /\.tex(?:$|[?#\s)])/i.test(String(value || ''));
}

function isTexNameLike(value) {
  return /\.tex$/i.test(String(value || '').trim());
}

function isTarGzNameLike(value) {
  return /\.tar\.gz$/i.test(String(value || '').trim()) || /\.tgz$/i.test(String(value || '').trim());
}

function candidateFileKind(candidate, targetName = '') {
  if (candidate?.fileKind) {
    return candidate.fileKind;
  }
  const haystack = `${candidate?.text || ''} ${candidate?.href || ''} ${candidate?.download || ''} ${candidate?.aria || ''} ${candidate?.title || ''} ${candidate?.label || ''}`;
  if (texNameLike(haystack) || (isTexNameLike(targetName) && /\b(tex|latex|chapter)\b/i.test(haystack))) {
    return 'downloaded-tex';
  }
  if (tarNameLike(haystack)) {
    return 'downloaded-archive';
  }
  if (targetName && !isTarGzNameLike(targetName)) {
    return isTexNameLike(targetName) ? 'downloaded-tex' : 'downloaded-file';
  }
  return 'downloaded-archive';
}

function normalizeTarComparable(value) {
  return normalizeArtifactComparable(value);
}

function normalizeArtifactComparable(value) {
  return String(value || '')
    .replace(/\.tar\(\d+\)\.gz/gi, '.tar.gz')
    .replace(/\.tgz/gi, '.tar.gz')
    .replace(/\s+/g, ' ')
    .toLowerCase();
}

async function readGenerationStatus(page) {
  return page.evaluate(() => {
    const controls = Array.from(document.querySelectorAll('button,[role="button"],[aria-label],[title]'));
    const visible = (el) => {
      const style = window.getComputedStyle(el);
      const rect = el.getBoundingClientRect();
      return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
    };
    const disabled = (el) => el.hasAttribute?.('disabled') || /^true$/i.test(el.getAttribute?.('aria-disabled') || '');
    const label = (el) => [
      el.innerText || el.textContent || '',
      el.getAttribute?.('aria-label') || '',
      el.getAttribute?.('title') || '',
    ].join(' ').replace(/\s+/g, ' ').trim();
    let activeStop = false;
    let finalActions = 0;
    let retryAvailable = false;
    for (const el of controls) {
      if (!visible(el) || disabled(el)) continue;
      const text = label(el);
      if (/\b(stop answering|stop generating|stop responding|stop thinking|stop)\b/i.test(text)) activeStop = true;
      if (/\b(copy response|good response|bad response|more actions|sources)\b/i.test(text)) finalActions += 1;
      if (/^\s*retry\s*$/i.test(text)) retryAvailable = true;
    }
    const pageText = String(document.body?.innerText || document.body?.textContent || '');
    const messageStreamError = /error in message stream/i.test(pageText);
    return { activeStop, finalActions, messageStreamError, retryAvailable };
  });
}

async function selectLongestABResponse(page) {
  return page.evaluate(() => {
    const pageText = document.body?.innerText || '';
    if (!/giving feedback on a new version|which response do you prefer/i.test(pageText)) {
      return { detected: false, selected: false, selectedIndex: -1, responseLengths: [] };
    }
    // Find response containers
    const abSelectors = [
      '[data-testid*="response-turn"]',
      '[data-testid*="response-option"]',
      '[class*="response-turn"]',
      '[class*="comparison"]',
    ];
    let responseRoots = [];
    for (const sel of abSelectors) {
      responseRoots = Array.from(document.querySelectorAll(sel));
      if (responseRoots.length >= 2) break;
    }
    if (responseRoots.length < 2) {
      return { detected: true, selected: false, selectedIndex: -1, responseLengths: [], reason: 'response-containers-not-found' };
    }
    const textOf = (el) => String(el?.innerText || el?.textContent || '').replace(/\s+/g, ' ').trim();
    const responseLengths = responseRoots.map((root) => textOf(root).length);
    // Find the longest response
    let longestIndex = 0;
    for (let i = 1; i < responseLengths.length; i++) {
      if (responseLengths[i] > responseLengths[longestIndex]) {
        longestIndex = i;
      }
    }
    // Try to click the longest response to select it
    const targetRoot = responseRoots[longestIndex];
    const clickTargets = Array.from(targetRoot.querySelectorAll('button,[role="button"],a'));
    const visible = (el) => {
      const style = window.getComputedStyle(el);
      const rect = el.getBoundingClientRect();
      return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
    };
    // Click the response container itself or a selectable element within it
    let selected = false;
    try {
      targetRoot.click();
      selected = true;
    } catch (e) {
      // Try clicking a button within
      for (const btn of clickTargets) {
        if (visible(btn)) {
          try {
            btn.click();
            selected = true;
            break;
          } catch (e2) { /* continue */ }
        }
      }
    }
    return { detected: true, selected, selectedIndex: longestIndex, responseLengths };
  });
}

async function downloadCandidate(page, candidate, outputDir, timeoutMs = 120000, context = {}) {
  const attempts = [];
  let lastError = null;
  const maxAttempts = 1;
  const browserDownloadDir = join(outputDir, '.browser-downloads');
  const diagnosticContext = { ...context, page: context.page ?? page, browserDownloadDir };
  for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
    const diagnostics = {
      attempt,
      clicked_at: timestamp(),
      candidate: downloadCandidateMetadata(candidate),
      suggested: '',
      target_path: '',
      download_failure: '',
      download_failure_error: '',
      download_temp_path: '',
      download_path_error: '',
      save_as_status: 'not-run',
      save_as_error: '',
      fallback_copy_status: 'not-run',
      fallback_copy_error: '',
      fallback_stream_status: 'not-run',
      fallback_stream_error: '',
      browser_download_status: 'not-run',
      browser_download_error: '',
      browser_download_path: '',
      browser_download_config_status: 'not-run',
      browser_download_config_error: '',
      persisted_by: '',
      error: '',
    };
    attempts.push(diagnostics);
    try {
      await configureBrowserDownloadDirectory(page, browserDownloadDir, diagnostics, diagnosticContext, candidate);
      const download = await triggerCandidateDownload(page, candidate, timeoutMs);
      const persisted = await persistPlaywrightDownload(download, candidate, outputDir, diagnostics, diagnosticContext);
      const inspected = await inspectDownloadedArtifact(persisted.targetPath);
      logDownloadDiagnostic(diagnosticContext, 'download-persisted', 'ok', 'download persisted and inspected', {
        ...downloadCandidateLogFields(candidate),
        attempt: String(attempt),
        suggested_filename: persisted.suggested,
        local_name: persisted.localName,
        target_path: persisted.targetPath,
        persisted_by: persisted.persistedBy,
        file_kind: inspected.fileKind,
        download_failure: diagnostics.download_failure,
        download_failure_error: diagnostics.download_failure_error,
        download_temp_path: diagnostics.download_temp_path,
        download_path_error: diagnostics.download_path_error,
        sha256: inspected.sha256,
        size_bytes: String(inspected.sizeBytes),
        entry_count: String(inspected.entryCount ?? ''),
      });
      return {
        path: persisted.targetPath,
        suggested: persisted.suggested,
        localName: persisted.localName,
        fileKind: inspected.fileKind,
        artifactKind: inspected.artifactKind,
        validationStatus: inspected.validationStatus,
        sizeBytes: inspected.sizeBytes,
        sha256: inspected.sha256,
        entryCount: inspected.entryCount,
        persistedBy: persisted.persistedBy,
      };
    } catch (error) {
      lastError = error;
      diagnostics.error = error?.message || String(error);
      break;
    }
  }

  const bundlePath = await writeDownloadFailureBundle(diagnosticContext, {
    kind: 'download-failed',
    message: lastError?.message || String(lastError || 'download failed'),
    output_dir: outputDir,
    candidate: downloadCandidateMetadata(candidate),
    attempts,
    error: lastError?.message || String(lastError || 'download failed'),
    created_at: timestamp(),
  });
  const error = lastError || new Error('download failed before a Playwright download was captured');
  if (bundlePath) {
    error.failureBundlePath = bundlePath;
    error.message = `${error.message}; diagnostics: ${bundlePath}`;
  }
  throw error;
}

async function materializeVisibleTextArtifact(page, targetName, outputDir, context = {}) {
  if (!isTextSafeArtifactName(targetName) || !page || page.isClosed?.()) {
    return null;
  }
  const extraction = await extractVisibleTextArtifact(page, targetName).catch((error) => ({
    error: error?.message || String(error),
  }));
  const content = selectMaterializableTextContent(targetName, extraction);
  if (!content) {
    logDownloadDiagnostic(context, 'artifact-text-materialization', 'skipped', 'no valid visible text artifact content found', {
      target_name: targetName,
      reason: extraction?.error || 'no-valid-content',
      candidate_count: String(extraction?.candidates?.length ?? 0),
      preview: compact(extraction?.assistantText || '', 160),
    }, 'warn');
    return null;
  }
  const localName = normalizeArtifactName(targetName);
  const targetPath = join(outputDir, localName);
  await mkdir(outputDir, { recursive: true });
  await writeFile(targetPath, content);
  let inspected;
  try {
    inspected = await inspectDownloadedArtifact(targetPath);
  } catch (error) {
    await rm(targetPath, { force: true }).catch(() => undefined);
    logDownloadDiagnostic(context, 'artifact-text-materialization', 'invalid', 'visible text artifact content failed validation', {
      target_name: targetName,
      target_path: targetPath,
      reason: error?.message || String(error),
      preview: compact(content, 160),
    }, 'warn');
    return null;
  }
  logDownloadDiagnostic(context, 'artifact-text-materialization', 'materialized', 'materialized text-safe artifact from assistant content', {
    target_name: targetName,
    target_path: targetPath,
    sha256: inspected.sha256,
    size_bytes: String(inspected.sizeBytes),
    artifact_kind: inspected.artifactKind,
    validation_status: inspected.validationStatus,
  }, 'warn');
  return {
    path: targetPath,
    suggested: localName,
    localName,
    fileKind: inspected.fileKind,
    artifactKind: inspected.artifactKind,
    validationStatus: inspected.validationStatus,
    sizeBytes: inspected.sizeBytes,
    sha256: inspected.sha256,
    entryCount: inspected.entryCount,
    persistedBy: 'materialized-from-text',
  };
}

async function submitArtifactRepairIfNeeded(bridge, tab, envelope, targetName, state, discovery = {}) {
  const limit = Math.max(0, Math.floor(Number(bridge.options.artifactRepairAttemptLimit) || 0));
  if (!targetName || limit <= 0 || state.attempts >= limit || tab.page?.isClosed?.()) {
    return { submitted: false, reason: limit <= 0 ? 'disabled' : 'limit-reached' };
  }
  const signal = await detectArtifactRepairSignal(tab.page, targetName, discovery).catch((error) => ({
    shouldRepair: false,
    reason: `detect-error:${error?.message || String(error)}`,
  }));
  if (!signal.shouldRepair) {
    return { submitted: false, reason: signal.reason || 'no-signal' };
  }
  const prompt = artifactRepairPrompt(targetName, signal);
  state.attempts += 1;
  bridge.bridgeLog(envelope, 'artifact-repair-submit', 'started', 'submitting artifact repair prompt', {
    target_name: targetName,
    attempt: String(state.attempts),
    max_attempts: String(limit),
    reason: signal.reason,
    preview: compact(signal.preview || '', 160),
  }, 'warn');
  try {
    await bridge.runDismissals(tab.page, envelope, 'artifact-repair-preflight');
    const result = await submitPromptToChat(tab.page, prompt, 45000, {
      dismiss: async (phase) => bridge.runDismissals(tab.page, envelope, phase),
      log: (phase, status, message, fields = {}, level = 'info') => {
        bridge.bridgeLog(envelope, phase, status, message, fields, level);
      },
      authState: (authState) => bridge.emitPromptAuthState(envelope, authState),
    });
    state.submitted = true;
    bridge.emit(envelope, 'artifact-repair-submitted', {
      target_name: targetName,
      attempt: state.attempts,
      reason: signal.reason,
    });
    bridge.bridgeLog(envelope, 'artifact-repair-submit', 'submitted', 'artifact repair prompt accepted by ChatGPT', {
      target_name: targetName,
      attempt: String(state.attempts),
      reason: signal.reason,
      acceptance_reason: result.acceptanceReason || '',
    }, 'warn');
    return { submitted: true, reason: signal.reason };
  } catch (error) {
    state.lastError = error?.message || String(error);
    bridge.bridgeLog(envelope, 'artifact-repair-submit', 'failed', 'artifact repair prompt failed', {
      target_name: targetName,
      attempt: String(state.attempts),
      reason: signal.reason,
      error: state.lastError,
    }, 'warn');
    return { submitted: false, reason: 'submit-failed', error: state.lastError };
  }
}

async function detectArtifactRepairSignal(page, targetName, discovery = {}) {
  const extraction = await extractVisibleTextArtifact(page, targetName);
  return artifactRepairSignalFromText(targetName, extraction.assistantText || discovery.lastTextPreview || '', extraction.candidates || []);
}

function artifactRepairSignalFromText(targetName, assistantText, candidates = []) {
  const target = normalizeArtifactComparable(targetName);
  const text = String(assistantText || '').trim();
  if (!target || !text || !normalizeArtifactComparable(text).includes(target)) {
    return { shouldRepair: false, reason: 'target-not-mentioned' };
  }
  if (selectMaterializableTextContent(targetName, { candidates: [...candidates, text] })) {
    return { shouldRepair: false, reason: 'valid-materializable-content' };
  }
  const escaped = escapeRegExp(targetName);
  const sandboxPattern = new RegExp(`sandbox:\\s*/?/?mnt/data/${escaped}`, 'i');
  const markdownPattern = new RegExp(`\\[\\s*${escaped}\\s*\\]\\s*\\(`, 'i');
  const compactText = text
    .replace(/\b(extended|copy response|good response|bad response|more actions)\b/gi, '')
    .replace(/\s+/g, ' ')
    .trim();
  const bareFilenamePattern = new RegExp(`^${escaped}\\.?$`, 'i');
  let reason = '';
  if (sandboxPattern.test(text)) {
    reason = 'malformed-sandbox-link';
  } else if (markdownPattern.test(text)) {
    reason = 'markdown-link-without-download';
  } else if (bareFilenamePattern.test(compactText)) {
    reason = 'filename-without-content';
  } else if (compactText.length <= targetName.length + 80) {
    reason = 'target-mentioned-without-content';
  }
  if (!reason) {
    return { shouldRepair: false, reason: 'no-repairable-artifact-intent' };
  }
  return {
    shouldRepair: true,
    reason,
    preview: compact(text, 240),
  };
}

function artifactRepairPrompt(targetName, signal) {
  const kind = artifactKindForPath(targetName);
  const textFallback = isTextSafeArtifactName(targetName)
    ? `If the UI cannot attach the file, provide the complete valid ${kind} contents in one fenced code block only.`
    : 'Attach the actual downloadable file; do not provide a filename, markdown link, or prose-only response.';
  return [
    `The previous response did not expose a clickable/downloadable artifact for ${targetName}.`,
    `Reason detected by automation: ${signal.reason}.`,
    `Create the artifact again with filename exactly ${targetName}.`,
    'Do not answer with a sandbox:/mnt/data markdown link or the filename alone.',
    textFallback,
  ].join('\n');
}

async function extractVisibleTextArtifact(page, targetName) {
  return page.evaluate(({ target }) => {
    const normalize = (value) => String(value || '').replace(/\s+/g, ' ').trim();
    const assistantRoots = Array.from(document.querySelectorAll('[data-message-author-role="assistant"]'));
    const root = assistantRoots.length > 0 ? assistantRoots[assistantRoots.length - 1] : document.body;
    const codeBlocks = Array.from(root.querySelectorAll('pre code, pre, code, textarea'))
      .map((element) => String(element.innerText || element.textContent || element.value || '').trim())
      .filter(Boolean);
    const assistantText = String(root?.innerText || root?.textContent || '').trim();
    const candidates = [...codeBlocks];
    if (assistantText) {
      candidates.push(assistantText);
      const lines = assistantText.split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
      const withoutTargetLines = lines
        .filter((line) => line !== target)
        .filter((line) => !/^(extended|copy response|good response|bad response|more actions)$/i.test(line))
        .join('\n')
        .trim();
      if (withoutTargetLines) {
        candidates.push(withoutTargetLines);
      }
    }
    return {
      assistantRootCount: assistantRoots.length,
      assistantText,
      candidates,
      target,
    };
  }, { target: targetName });
}

function selectMaterializableTextContent(targetName, extraction = {}) {
  const kind = artifactKindForPath(targetName);
  for (const raw of extraction?.candidates || []) {
    const content = normalizeMaterializedTextContent(raw, targetName, kind);
    if (content && isValidMaterializedTextContent(content, kind, targetName)) {
      return content;
    }
  }
  return '';
}

function normalizeMaterializedTextContent(raw, targetName, kind) {
  let content = String(raw || '').trim();
  if (!content || content === targetName) {
    return '';
  }
  content = content
    .replace(/^```[A-Za-z0-9_-]*\s*/i, '')
    .replace(/\s*```$/i, '')
    .trim();
  if (kind === 'json') {
    const objectStart = content.indexOf('{');
    const arrayStart = content.indexOf('[');
    const startCandidates = [objectStart, arrayStart].filter((index) => index >= 0);
    if (startCandidates.length > 0) {
      const start = Math.min(...startCandidates);
      const end = Math.max(content.lastIndexOf('}'), content.lastIndexOf(']'));
      if (end > start) {
        content = content.slice(start, end + 1).trim();
      }
    }
  }
  return content === targetName ? '' : content;
}

function isValidMaterializedTextContent(content, kind, targetName) {
  if (!content.trim() || content.trim() === targetName) {
    return false;
  }
  try {
    if (kind === 'json') {
      JSON.parse(content);
      return true;
    }
    if (kind === 'jsonl') {
      const lines = content.split(/\r?\n/).filter((line) => line.trim());
      if (lines.length === 0) return false;
      for (const line of lines) JSON.parse(line);
      return true;
    }
    if (['markdown', 'text', 'csv', 'tsv', 'tex'].includes(kind)) {
      return true;
    }
  } catch {
    return false;
  }
  return false;
}

function isTextSafeArtifactName(targetName) {
  return ['json', 'jsonl', 'markdown', 'text', 'csv', 'tsv', 'tex'].includes(artifactKindForPath(targetName));
}

async function triggerCandidateDownload(page, candidate, timeoutMs) {
  const downloadPromise = page.waitForEvent('download', { timeout: timeoutMs });
  const locator = page.locator('a,button,[role="button"],[download],[href]').nth(candidate.index);
  await locator.scrollIntoViewIfNeeded({ timeout: 5000 }).catch(() => undefined);
  await locator.click({ timeout: timeoutMs });
  return downloadPromise;
}

async function configureBrowserDownloadDirectory(page, outputDir, diagnostics, context, candidate) {
  await mkdir(outputDir, { recursive: true });
  const pageContext = typeof page?.context === 'function' ? page.context() : null;
  if (!pageContext || typeof pageContext.newCDPSession !== 'function') {
    diagnostics.browser_download_config_status = 'skipped';
    diagnostics.browser_download_config_error = 'page context CDP session unavailable';
    return;
  }

  let session = null;
  try {
    session = await pageContext.newCDPSession(page);
    try {
      await session.send('Browser.setDownloadBehavior', {
        behavior: 'allow',
        downloadPath: outputDir,
        eventsEnabled: true,
      });
      diagnostics.browser_download_config_status = 'browser-ok';
    } catch (browserError) {
      await session.send('Page.setDownloadBehavior', {
        behavior: 'allow',
        downloadPath: outputDir,
      });
      diagnostics.browser_download_config_status = 'page-ok';
      diagnostics.browser_download_config_error = browserError?.message || String(browserError);
    }
    logDownloadDiagnostic(context, 'download-directory-configured', diagnostics.browser_download_config_status, 'browser download directory configured', {
      ...downloadCandidateLogFields(candidate),
      attempt: String(diagnostics.attempt),
      download_dir: outputDir,
      fallback_reason: diagnostics.browser_download_config_error,
    });
  } catch (error) {
    diagnostics.browser_download_config_status = 'failed';
    diagnostics.browser_download_config_error = error?.message || String(error);
    logDownloadDiagnostic(context, 'download-directory-configured', 'failed', 'browser download directory configuration failed', {
      ...downloadCandidateLogFields(candidate),
      attempt: String(diagnostics.attempt),
      download_dir: outputDir,
      reason: diagnostics.browser_download_config_error,
    }, 'warn');
  } finally {
    if (session && typeof session.detach === 'function') {
      await session.detach().catch(() => undefined);
    }
  }
}

async function persistPlaywrightDownload(download, candidate, outputDir, diagnostics, context) {
  const suggested = normalizeArtifactName(download.suggestedFilename() || basename(candidate.href || '') || 'chatgpt-output.tar.gz');
  const localName = context?.targetName ? normalizeArtifactName(context.targetName) : suggested;
  const targetPath = join(outputDir, localName);
  diagnostics.suggested = suggested;
  diagnostics.target_path = targetPath;
  await mkdir(outputDir, { recursive: true });
  try {
    await download.saveAs(targetPath);
    const savedTarget = await persistExistingTargetIfUsable(targetPath, diagnostics, context, candidate, 'saveAs');
    if (!savedTarget.ok) {
      throw new Error(`artifact saveAs did not create a usable file: ${savedTarget.error}`);
    }
    diagnostics.save_as_status = 'ok';
  } catch (error) {
    diagnostics.save_as_status = 'failed';
    diagnostics.save_as_error = error?.message || String(error);
    const partialTargetPersist = await persistExistingTargetIfUsable(targetPath, diagnostics, context, candidate, 'saveAs-partial');
    if (partialTargetPersist.ok) {
      diagnostics.persisted_by = partialTargetPersist.method;
      return { targetPath, suggested, localName, persistedBy: partialTargetPersist.method };
    }
    const failure = await safeDownloadFailure(download);
    const tempPath = await safeDownloadPath(download);
    diagnostics.download_failure = failure.value;
    diagnostics.download_failure_error = failure.error;
    diagnostics.download_temp_path = tempPath.value;
    diagnostics.download_path_error = tempPath.error;
    logDownloadDiagnostic(context, 'download-save-failed', 'failed', 'Playwright artifact saveAs failed', {
      ...downloadCandidateLogFields(candidate),
      attempt: String(diagnostics.attempt),
      suggested_filename: suggested,
      target_path: targetPath,
      download_failure: diagnostics.download_failure,
      download_failure_error: diagnostics.download_failure_error,
      download_temp_path: diagnostics.download_temp_path,
      download_path_error: diagnostics.download_path_error,
      reason: diagnostics.save_as_error,
    }, 'warn');
    const tempPathPersist = await persistDownloadFromTempPath(tempPath.value, targetPath, diagnostics, context, candidate);
    if (tempPathPersist.ok) {
      diagnostics.persisted_by = tempPathPersist.method;
      return { targetPath, suggested, localName, persistedBy: tempPathPersist.method };
    }
    const browserDownloadPersist = await persistDownloadFromBrowserDownloads(
      suggested,
      targetPath,
      diagnostics,
      context,
      candidate,
    );
    if (browserDownloadPersist.ok) {
      diagnostics.persisted_by = browserDownloadPersist.method;
      return { targetPath, suggested, localName, persistedBy: browserDownloadPersist.method };
    }
    const finalTargetPersist = await persistExistingTargetIfUsable(targetPath, diagnostics, context, candidate, 'recovered-target');
    if (finalTargetPersist.ok) {
      diagnostics.persisted_by = finalTargetPersist.method;
      return { targetPath, suggested, localName, persistedBy: finalTargetPersist.method };
    }
    const saveError = new Error([
      'artifact saveAs failed',
      `target-path recovery failed: ${[partialTargetPersist.error, finalTargetPersist.error].filter(Boolean).join('; ') || 'not usable'}`,
      `temp-path recovery failed: ${tempPathPersist.error || diagnostics.save_as_error}`,
      `browser-download recovery failed: ${browserDownloadPersist.error || diagnostics.browser_download_error || 'not found'}`,
    ].join('; '));
    saveError.saveAsFailed = true;
    throw saveError;
  }

  const failure = await safeDownloadFailure(download);
  const tempPath = await safeDownloadPath(download);
  diagnostics.download_failure = failure.value;
  diagnostics.download_failure_error = failure.error;
  diagnostics.download_temp_path = tempPath.value;
  diagnostics.download_path_error = tempPath.error;
  diagnostics.persisted_by = 'saveAs';
  if (failure.value) {
    throw new Error(`download failed: ${failure.value}`);
  }
  return { targetPath, suggested, localName, persistedBy: 'saveAs' };
}

async function persistExistingTargetIfUsable(targetPath, diagnostics, context, candidate, method) {
  let fileStat = null;
  try {
    fileStat = await stat(targetPath);
  } catch (error) {
    return { ok: false, method: '', error: error?.message || String(error) };
  }

  if (fileStat.isFile() && fileStat.size > 0) {
    logDownloadDiagnostic(context, 'download-target-path-persist', 'accepted', 'existing artifact target path is usable', {
      ...downloadCandidateLogFields(candidate),
      attempt: String(diagnostics.attempt),
      target_path: targetPath,
      size_bytes: String(fileStat.size),
      method,
    });
    return { ok: true, method, error: '' };
  }

  const reason = fileStat.isFile()
    ? `target path is empty: ${targetPath}`
    : `target path is not a file: ${targetPath}`;
  try {
    await rm(targetPath, { force: true });
  } catch (error) {
    const cleanupError = error?.message || String(error);
    logDownloadDiagnostic(context, 'download-target-path-persist', 'cleanup-failed', 'unusable artifact target cleanup failed', {
      ...downloadCandidateLogFields(candidate),
      attempt: String(diagnostics.attempt),
      target_path: targetPath,
      reason,
      cleanup_error: cleanupError,
      method,
    }, 'warn');
    return { ok: false, method: '', error: `${reason}; cleanup failed: ${cleanupError}` };
  }
  logDownloadDiagnostic(context, 'download-target-path-persist', 'removed-empty', 'removed unusable artifact target path', {
    ...downloadCandidateLogFields(candidate),
    attempt: String(diagnostics.attempt),
    target_path: targetPath,
    reason,
    method,
  }, 'warn');
  return { ok: false, method: '', error: reason };
}

async function persistDownloadFromTempPath(tempPath, targetPath, diagnostics, context, candidate) {
  if (!tempPath) {
    diagnostics.fallback_copy_status = 'skipped';
    diagnostics.fallback_stream_status = 'skipped';
    const error = diagnostics.download_path_error || 'download.path unavailable';
    logDownloadDiagnostic(context, 'download-temp-path-persist', 'skipped', 'download temp path unavailable for persistence', {
      ...downloadCandidateLogFields(candidate),
      attempt: String(diagnostics.attempt),
      target_path: targetPath,
      download_path_error: error,
    }, 'warn');
    return { ok: false, method: '', error };
  }

  try {
    await copyFile(tempPath, targetPath);
    diagnostics.fallback_copy_status = 'ok';
    logDownloadDiagnostic(context, 'download-temp-path-persist', 'copied', 'copied Playwright temp download to target path', {
      ...downloadCandidateLogFields(candidate),
      attempt: String(diagnostics.attempt),
      target_path: targetPath,
      download_temp_path: tempPath,
      method: 'copyFile',
    }, 'warn');
    return { ok: true, method: 'temp-path-copy', error: '' };
  } catch (error) {
    diagnostics.fallback_copy_status = 'failed';
    diagnostics.fallback_copy_error = error?.message || String(error);
    logDownloadDiagnostic(context, 'download-temp-path-persist', 'copy-failed', 'copying Playwright temp download failed', {
      ...downloadCandidateLogFields(candidate),
      attempt: String(diagnostics.attempt),
      target_path: targetPath,
      download_temp_path: tempPath,
      reason: diagnostics.fallback_copy_error,
      method: 'copyFile',
    }, 'warn');
  }

  try {
    await pipeline(createReadStream(tempPath), createWriteStream(targetPath));
    diagnostics.fallback_stream_status = 'ok';
    logDownloadDiagnostic(context, 'download-temp-path-persist', 'streamed', 'streamed Playwright temp download to target path', {
      ...downloadCandidateLogFields(candidate),
      attempt: String(diagnostics.attempt),
      target_path: targetPath,
      download_temp_path: tempPath,
      method: 'stream',
    }, 'warn');
    return { ok: true, method: 'temp-path-stream', error: '' };
  } catch (error) {
    diagnostics.fallback_stream_status = 'failed';
    diagnostics.fallback_stream_error = error?.message || String(error);
    logDownloadDiagnostic(context, 'download-temp-path-persist', 'stream-failed', 'streaming Playwright temp download failed', {
      ...downloadCandidateLogFields(candidate),
      attempt: String(diagnostics.attempt),
      target_path: targetPath,
      download_temp_path: tempPath,
      reason: diagnostics.fallback_stream_error,
      method: 'stream',
    }, 'warn');
  }

  return {
    ok: false,
    method: '',
    error: [diagnostics.fallback_copy_error, diagnostics.fallback_stream_error].filter(Boolean).join('; '),
  };
}

async function persistDownloadFromBrowserDownloads(suggested, targetPath, diagnostics, context, candidate) {
  const sinceMs = Date.parse(diagnostics.clicked_at || '') || 0;
  const profileDownloadsDir = context?.browserProfileDir ? join(context.browserProfileDir, 'Downloads') : '';
  const dirs = browserDownloadRecoveryDirs([
    context?.browserDownloadDir || '',
    dirname(targetPath),
    context?.downloadsDir || '',
    profileDownloadsDir,
  ]);
  const matches = [];
  for (const dir of dirs) {
    let entries = [];
    try {
      entries = await readdir(dir, { withFileTypes: true });
    } catch (error) {
      diagnostics.browser_download_error = diagnostics.browser_download_error || `${dir}: ${error?.message || String(error)}`;
      continue;
    }
    for (const entry of entries) {
      if (!entry.isFile() || !isSuggestedDownloadName(entry.name, suggested)) {
        continue;
      }
      const candidatePath = join(dir, entry.name);
      try {
        const fileStat = await stat(candidatePath);
        if (fileStat.size <= 0) {
          continue;
        }
        if (sinceMs && fileStat.mtimeMs + 5000 < sinceMs) {
          continue;
        }
        matches.push({ path: candidatePath, size: fileStat.size, mtimeMs: fileStat.mtimeMs });
      } catch (error) {
        diagnostics.browser_download_error = diagnostics.browser_download_error || `${candidatePath}: ${error?.message || String(error)}`;
      }
    }
  }

  matches.sort((a, b) => b.mtimeMs - a.mtimeMs);
  const match = matches[0];
  if (!match) {
    diagnostics.browser_download_status = 'missing';
    const error = diagnostics.browser_download_error || `no fresh ${suggested} found in browser download dirs`;
    diagnostics.browser_download_error = error;
    logDownloadDiagnostic(context, 'download-browser-download-persist', 'missing', 'browser download file unavailable for persistence', {
      ...downloadCandidateLogFields(candidate),
      attempt: String(diagnostics.attempt),
      target_path: targetPath,
      suggested_filename: suggested,
      searched_dirs: dirs.join(','),
      reason: error,
    }, 'warn');
    return { ok: false, method: '', error };
  }

  try {
    await copyFile(match.path, targetPath);
    diagnostics.browser_download_status = 'copied';
    diagnostics.browser_download_path = match.path;
    logDownloadDiagnostic(context, 'download-browser-download-persist', 'copied', 'copied browser download file to target path', {
      ...downloadCandidateLogFields(candidate),
      attempt: String(diagnostics.attempt),
      target_path: targetPath,
      browser_download_path: match.path,
      size_bytes: String(match.size),
      method: 'copyFile',
    }, 'warn');
    return { ok: true, method: 'browser-download-copy', error: '' };
  } catch (error) {
    diagnostics.browser_download_status = 'copy-failed';
    diagnostics.browser_download_path = match.path;
    diagnostics.browser_download_error = error?.message || String(error);
    logDownloadDiagnostic(context, 'download-browser-download-persist', 'copy-failed', 'copying browser download file failed', {
      ...downloadCandidateLogFields(candidate),
      attempt: String(diagnostics.attempt),
      target_path: targetPath,
      browser_download_path: match.path,
      reason: diagnostics.browser_download_error,
      method: 'copyFile',
    }, 'warn');
    return { ok: false, method: '', error: diagnostics.browser_download_error };
  }
}

function browserDownloadRecoveryDirs(extraDirs = []) {
  const dirs = [
    ...extraDirs,
    process.env.JAILGUN_BROWSER_DOWNLOADS_DIR || '',
    process.env.XDG_DOWNLOAD_DIR || '',
    join(homedir(), 'Downloads'),
  ].filter(Boolean);
  return [...new Set(dirs.map((dir) => resolvePath(dir)))];
}

function isSuggestedDownloadName(name, suggested) {
  if (name === suggested) {
    return true;
  }
  if (suggested.endsWith('.tar.gz')) {
    const stem = suggested.slice(0, -'.tar.gz'.length);
    return new RegExp(`^${escapeRegExp(stem)} \\(\\d+\\)\\.tar\\.gz$`).test(name);
  }
  const extension = extname(suggested);
  const stem = extension ? suggested.slice(0, -extension.length) : suggested;
  return new RegExp(`^${escapeRegExp(stem)} \\(\\d+\\)${escapeRegExp(extension)}$`).test(name);
}

function escapeRegExp(value) {
  return String(value).replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

async function safeDownloadFailure(download) {
  try {
    return { value: await download.failure() || '', error: '' };
  } catch (error) {
    return { value: '', error: error?.message || String(error) };
  }
}

async function safeDownloadPath(download) {
  try {
    return { value: await download.path() || '', error: '' };
  } catch (error) {
    return { value: '', error: error?.message || String(error) };
  }
}

async function inspectDownloadedArtifact(filePath) {
  const fileStat = await stat(filePath);
  if (!fileStat.isFile() || fileStat.size === 0) {
    throw new Error(`downloaded file was empty or not a file: ${filePath}`);
  }
  const sha256 = await sha256File(filePath);
  const artifactKind = artifactKindForPath(filePath);
  if (artifactKind !== 'tar.gz') {
    await validateDownloadedFileByKind(filePath, artifactKind);
    return {
      sizeBytes: fileStat.size,
      sha256,
      entryCount: null,
      fileKind: artifactKind === 'tex' ? 'downloaded-tex' : 'downloaded-file',
      artifactKind,
      validationStatus: 'ok',
    };
  }
  const tarList = spawnSync('tar', ['-tzf', filePath], { encoding: 'utf8' });
  if (tarList.status !== 0) {
    throw new Error(`downloaded file is not a valid tar.gz: ${tarList.stderr.trim()}`);
  }
  const entryCount = tarList.stdout.split(/\r?\n/).map((line) => line.trim()).filter(Boolean).length;
  if (entryCount === 0) {
    throw new Error(`downloaded file has zero tar entries: ${filePath}`);
  }
  return {
    sizeBytes: fileStat.size,
    sha256,
    entryCount,
    fileKind: 'downloaded-archive',
    artifactKind,
    validationStatus: 'ok',
  };
}

function artifactKindForPath(filePath) {
  const lower = String(filePath || '').toLowerCase();
  if (lower.endsWith('.tar.gz') || lower.endsWith('.tgz')) return 'tar.gz';
  if (lower.endsWith('.json')) return 'json';
  if (lower.endsWith('.jsonl')) return 'jsonl';
  if (lower.endsWith('.md') || lower.endsWith('.markdown')) return 'markdown';
  if (lower.endsWith('.txt')) return 'text';
  if (lower.endsWith('.csv')) return 'csv';
  if (lower.endsWith('.tsv')) return 'tsv';
  if (lower.endsWith('.tex')) return 'tex';
  if (lower.endsWith('.pdf')) return 'pdf';
  if (/\.(png|jpe?g|webp|gif|svg)$/i.test(lower)) return 'image';
  return 'unknown';
}

async function validateDownloadedFileByKind(filePath, artifactKind) {
  const bytes = await readFile(filePath);
  if (bytes.length === 0) {
    throw new Error(`downloaded ${artifactKind} artifact was empty: ${filePath}`);
  }
  if (artifactKind === 'json') {
    JSON.parse(bytes.toString('utf8'));
    return;
  }
  if (artifactKind === 'jsonl') {
    const lines = bytes.toString('utf8').split(/\r?\n/).filter((line) => line.trim());
    if (lines.length === 0) {
      throw new Error(`downloaded jsonl artifact had no records: ${filePath}`);
    }
    for (const line of lines) {
      JSON.parse(line);
    }
    return;
  }
  if (['markdown', 'text', 'csv', 'tsv', 'tex'].includes(artifactKind)) {
    const text = bytes.toString('utf8');
    if (!text.trim()) {
      throw new Error(`downloaded ${artifactKind} artifact contained only whitespace: ${filePath}`);
    }
    return;
  }
  if (artifactKind === 'pdf') {
    if (!bytes.subarray(0, 5).equals(Buffer.from('%PDF-'))) {
      throw new Error(`downloaded PDF artifact had invalid magic bytes: ${filePath}`);
    }
    return;
  }
  if (artifactKind === 'image') {
    if (!looksLikeImageBytes(bytes, filePath)) {
      throw new Error(`downloaded image artifact had invalid magic bytes: ${filePath}`);
    }
  }
}

function looksLikeImageBytes(bytes, filePath) {
  const lower = String(filePath || '').toLowerCase();
  if (lower.endsWith('.svg')) {
    return /<svg[\s>]/i.test(bytes.toString('utf8', 0, Math.min(bytes.length, 4096)));
  }
  if (lower.endsWith('.png')) {
    return bytes.length >= 8 && bytes[0] === 0x89 && bytes[1] === 0x50 && bytes[2] === 0x4e && bytes[3] === 0x47;
  }
  if (lower.endsWith('.jpg') || lower.endsWith('.jpeg')) {
    return bytes.length >= 3 && bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff;
  }
  if (lower.endsWith('.gif')) {
    return bytes.subarray(0, 3).toString('ascii') === 'GIF';
  }
  if (lower.endsWith('.webp')) {
    return bytes.length >= 12 && bytes.subarray(0, 4).toString('ascii') === 'RIFF' && bytes.subarray(8, 12).toString('ascii') === 'WEBP';
  }
  return true;
}

function downloadCandidateMetadata(candidate) {
  return {
    index: candidate?.index ?? null,
    score: candidate?.score ?? null,
    label: compact(candidate?.label || '', 240),
    text: compact(candidate?.text || '', 240),
    href: compact(candidate?.href || '', 240),
    download: compact(candidate?.download || '', 240),
    aria: compact(candidate?.aria || '', 160),
    title: compact(candidate?.title || '', 160),
    tag: candidate?.tag || '',
    role: candidate?.role || '',
    assistantIndex: candidate?.assistantIndex ?? null,
    fileKind: candidate?.fileKind || '',
  };
}

function downloadCandidateLogFields(candidate) {
  const meta = downloadCandidateMetadata(candidate);
  return {
    candidate_index: String(meta.index ?? ''),
    candidate_score: String(meta.score ?? ''),
    candidate_label: meta.label,
    candidate_href: meta.href,
    candidate_download: meta.download,
    candidate_tag: meta.tag,
    candidate_role: meta.role,
    candidate_assistant_index: meta.assistantIndex == null ? '' : String(meta.assistantIndex),
    candidate_file_kind: meta.fileKind,
  };
}

function logDownloadDiagnostic(context, phase, status, message, fields = {}, level = 'info') {
  if (!context?.bridge || !context?.envelope) {
    return;
  }
  context.bridge.bridgeLog(context.envelope, phase, status, message, fields, level);
}

async function writeDownloadFailureBundle(context, payload) {
  const bundle = await writeDownloadTroubleshootingBundle(context, payload, {
    logPhase: 'download-failure-bundle',
    logMessage: 'download failure diagnostics bundle written',
  });
  return bundle.bundleDir;
}

async function writeNoLinkBundle(context, payload) {
  const bundle = await writeDownloadTroubleshootingBundle(context, payload, {
    logPhase: 'no-link-bundle',
    logMessage: 'no-link page bundle written',
  });
  return bundle.bundleDir;
}

async function writeDownloadTroubleshootingBundle(context, payload, options = {}) {
  const artifactsDir = context?.artifactsDir;
  if (!artifactsDir) {
    return { bundleDir: '', snapshotPath: '' };
  }
  const runId = sanitizePathSegment(context?.envelope?.run_id || 'unknown-run');
  const tabName = `tab-${String(context?.tabId ?? 'unknown').padStart(2, '0')}`;
  const kind = sanitizePathSegment(payload?.kind || 'download-failed');
  const bundleDir = join(artifactsDir, 'download-failures', runId, tabName, `${pathTimestamp()}-${kind}`);
  const snapshotPath = join(bundleDir, 'snapshot.json');
  const htmlPath = join(bundleDir, 'page.html');
  const textPath = join(bundleDir, 'page.txt');
  const screenshotPath = join(bundleDir, 'page.png');
  const discoveryPath = join(bundleDir, 'candidate-discovery.json');
  const attemptsPath = join(bundleDir, 'download-attempts.json');
  const logPhase = options.logPhase || 'download-failure-bundle';
  try {
    await mkdir(bundleDir, { recursive: true });
    const page = context?.page;
    const pageUrl = payload?.pageUrl || (page && !page.isClosed() && typeof page.url === 'function' ? page.url() : '');
    let pageTitle = '';
    let pageHtml = '';
    let pageText = '';
    let discovery = null;
    let screenshotError = '';
    if (page && !page.isClosed()) {
      try {
        pageTitle = await page.title();
      } catch {
        pageTitle = '';
      }
      try {
        pageHtml = await page.content();
      } catch {
        pageHtml = '';
      }
      try {
        const rawText = await page.evaluate(() => String(document.body?.innerText || ''));
        pageText = typeof rawText === 'string' ? rawText : JSON.stringify(rawText, null, 2);
      } catch {
        pageText = '';
      }
      try {
        discovery = await discoverTarCandidates(page, artifactTargetName(context?.bridge?.options || {}));
      } catch (error) {
        discovery = {
          error: error?.message || String(error),
        };
      }
      try {
        await page.screenshot({ path: screenshotPath, fullPage: true });
      } catch (error) {
        screenshotError = error?.message || String(error);
      }
    }
    const attempts = payload?.attempts || payload?.downloadAttempts || [];
    await writeFile(htmlPath, pageHtml);
    await writeFile(textPath, pageText);
    await writeFile(discoveryPath, JSON.stringify(redactDiagnosticJson(discovery || {}), null, 2));
    await writeFile(attemptsPath, JSON.stringify(redactDiagnosticJson(attempts), null, 2));
    const snapshot = {
      captured_at: timestamp(),
      run_id: context?.envelope?.run_id || '',
      tab_id: context?.tabId ?? null,
      kind: payload?.kind || '',
      message: payload?.message || '',
      error: payload?.error || '',
      page_url: pageUrl,
      page_title: pageTitle,
      bundle_dir: bundleDir,
      html_path: htmlPath,
      text_path: textPath,
      screenshot_path: existsSync(screenshotPath) ? screenshotPath : '',
      candidate_discovery_path: discoveryPath,
      download_attempts_path: attemptsPath,
      html_bytes: Buffer.byteLength(pageHtml, 'utf8'),
      text_bytes: Buffer.byteLength(pageText, 'utf8'),
      discovery_bytes: Buffer.byteLength(JSON.stringify(discovery || {}), 'utf8'),
      download_attempt_count: Array.isArray(attempts) ? attempts.length : 0,
      candidate: redactDiagnosticJson(payload?.candidate || null),
      output_dir: payload?.output_dir || '',
      details: redactDiagnosticJson(payload?.details || {}),
      screenshot_error: screenshotError,
    };
    await writeFile(snapshotPath, JSON.stringify(snapshot, null, 2));
    logDownloadDiagnostic(context, logPhase, 'written', options.logMessage || 'download troubleshooting bundle written', {
      path: bundleDir,
      bundle_dir: bundleDir,
      snapshot_path: snapshotPath,
      html_path: htmlPath,
      text_path: textPath,
    }, 'warn');
    return { bundleDir, snapshotPath };
  } catch (error) {
    logDownloadDiagnostic(context, logPhase, 'failed', 'failed to write download troubleshooting bundle', {
      path: bundleDir,
      snapshot_path: snapshotPath,
      reason: error?.message || String(error),
    }, 'error');
    return { bundleDir: '', snapshotPath: '' };
  }
}

function pathTimestamp() {
  return timestamp().replace(/[:.]/g, '-');
}

function redactDiagnosticJson(value) {
  if (Array.isArray(value)) {
    return value.map((item) => redactDiagnosticJson(item));
  }
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, redactDiagnosticJson(item)]));
  }
  if (typeof value === 'string') {
    return redactSensitiveText(value);
  }
  return value;
}

async function dismissRateLimitModal(page) {
  try {
    return await page.evaluate(() => {
      const dialogSelector = '[role="dialog"],[aria-modal="true"]';
      const buttonSelector = 'button,[role="button"],a';
      const primary = /too many requests|making requests too quickly|temporarily limited access/i;
      const secondary = /please wait a few minutes|wait a few minutes before trying again/i;
      const buttonLabel = /^\s*got it\s*$/i;
      const visible = (el) => {
        const view = el.ownerDocument && el.ownerDocument.defaultView;
        if (!view) return true;
        const style = view.getComputedStyle(el);
        const rect = el.getBoundingClientRect();
        return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
      };
      const disabled = (el) => el.hasAttribute('disabled') || /^true$/i.test(el.getAttribute('aria-disabled') || '');
      const textOf = (el) => String(el.textContent || '').replace(/\s+/g, ' ').trim();
      const dialogs = Array.from(document.querySelectorAll(dialogSelector));
      for (const dialog of dialogs) {
        if (!visible(dialog)) continue;
        const dialogText = textOf(dialog);
        if (!primary.test(dialogText) || !secondary.test(dialogText)) continue;
        const buttons = Array.from(dialog.querySelectorAll(buttonSelector));
        for (const button of buttons) {
          if (!visible(button) || disabled(button)) continue;
          const label = textOf(button) || button.getAttribute('aria-label') || button.getAttribute('title') || '';
          if (!buttonLabel.test(label)) continue;
          button.click();
          return {
            detected: true,
            dismissed: true,
            excerpt: dialogText.slice(0, 240),
            buttonLabel: label,
          };
        }
        return {
          detected: true,
          dismissed: false,
          excerpt: dialogText.slice(0, 240),
          buttonLabel: '',
          reason: 'no-got-it-button',
        };
      }
      return { detected: false, dismissed: false, excerpt: '', buttonLabel: '' };
    });
  } catch (error) {
    return {
      detected: false,
      dismissed: false,
      excerpt: '',
      buttonLabel: '',
      reason: `evaluate-failed: ${error.message}`,
    };
  }
}

async function dismissPopups(page) {
  try {
    return await page.evaluate(() => {
      const dialogSelector = '[role="dialog"],[aria-modal="true"],[data-testid*="modal"],[data-testid*="dialog"]';
      const buttonSelector = 'button,[role="button"],a';
      const normalize = (value) => String(value || '').replace(/\s+/g, ' ').trim();
      const visible = (el) => {
        const view = el.ownerDocument && el.ownerDocument.defaultView;
        if (!view) return true;
        const style = view.getComputedStyle(el);
        const rect = el.getBoundingClientRect();
        return style.visibility !== 'hidden' && style.display !== 'none' && rect.width >= 0 && rect.height >= 0;
      };
      const disabled = (el) => el.hasAttribute('disabled') || /^true$/i.test(el.getAttribute('aria-disabled') || '');
      const textOf = (el) => normalize(el.innerText || el.textContent || '');
      const labelOf = (el) => normalize(textOf(el) || el.getAttribute('aria-label') || el.getAttribute('title') || '');
      const dialogs = Array.from(document.querySelectorAll(dialogSelector)).filter(visible);
      for (const dialog of dialogs) {
        const dialogText = textOf(dialog);
        if (/session expired|sign in again|log in again/i.test(dialogText)) {
          return {
            detected: true,
            clicked: false,
            kind: 'session-expired',
            excerpt: dialogText.slice(0, 240),
            label: '',
            reason: 'session expired prompt detected',
          };
        }
        if (/leave site|leave page|unsaved|changes you made|stay on page/i.test(dialogText)) {
          const buttons = Array.from(dialog.querySelectorAll(buttonSelector));
          for (const button of buttons) {
            if (!visible(button) || disabled(button)) continue;
            const label = labelOf(button);
            if (/stay|cancel|keep|continue editing/i.test(label)) {
              button.click();
              return {
                detected: true,
                clicked: true,
                kind: 'stay-on-page',
                excerpt: dialogText.slice(0, 240),
                label,
                reason: '',
              };
            }
          }
          return {
            detected: true,
            clicked: false,
            kind: 'stay-on-page',
            excerpt: dialogText.slice(0, 240),
            label: '',
            reason: 'safe-button-not-found',
          };
        }
      }
      return { detected: false, clicked: false, kind: '', excerpt: '', label: '', reason: '' };
    });
  } catch (error) {
    return {
      detected: false,
      clicked: false,
      kind: '',
      excerpt: '',
      label: '',
      reason: `evaluate-failed: ${error.message}`,
    };
  }
}

async function handleGitHubToolPrompt(page) {
  try {
    return await page.evaluate(() => {
      const controlSelector = 'button,[role="button"],a';
      const normalize = (value) => String(value || '').replace(/\s+/g, ' ').trim();
      const visible = (el) => {
        const style = window.getComputedStyle(el);
        const rect = el.getBoundingClientRect();
        return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
      };
      const disabled = (el) => el.hasAttribute?.('disabled') || /^true$/i.test(el.getAttribute?.('aria-disabled') || '');
      const textOf = (el) => normalize(el?.innerText || el?.textContent || '');
      const labelOf = (el) => normalize(textOf(el) || el?.getAttribute?.('aria-label') || el?.getAttribute?.('title') || '');
      const signatureFor = (text) => normalize(text).toLowerCase().slice(0, 240);
      const repositoryFrom = (text) => {
        const match = text.match(/[A-Za-z0-9_.-]+\s*\/\s*[A-Za-z0-9_.-]+/);
        return match ? match[0].replace(/\s+/g, '') : '';
      };
      const approvalLabel = /\b(allow|approve|authorize|continue|connect|grant|enable access)\b/i;
      const denialLabel = /^(deny|cancel|dismiss|not now|no thanks)$/i;
      const promptContext = /github|git\s*hub/i;
      const permissionContext = /\b(access|authorize|authorization|permission|permissions|connect|connection|grant|repository|repositories|repo|tool|connector|app)\b/i;
      const disallowedContainers = /^(body|main|nav|aside|html)$/i;
      const controls = Array.from(document.querySelectorAll(controlSelector))
        .filter((el) => visible(el) && !disabled(el))
        .map((el) => ({ el, label: labelOf(el) }))
        .filter((item) => approvalLabel.test(item.label) || denialLabel.test(item.label));
      for (const { el: control, label: seedLabel } of controls) {
        let node = control.parentElement || control;
        for (let depth = 0; node && depth < 8; depth += 1) {
          if (disallowedContainers.test(String(node.tagName || ''))) {
            break;
          }
          const context = textOf(node);
          if (context.length > 20 && context.length <= 2400 && promptContext.test(context) && permissionContext.test(context)) {
            const scopedControls = Array.from(node.querySelectorAll(controlSelector)).filter((el) => visible(el) && !disabled(el));
            const denial = scopedControls
              .map((el, index) => ({ el, index, label: labelOf(el) }))
              .find((item) => denialLabel.test(item.label));
            if (!denial) {
              return {
                detected: true,
                clicked: false,
                decision: 'deny',
                reason: 'deny-control-not-found',
                candidate: {
                  signature: signatureFor(context),
                  label: seedLabel,
                  repository: repositoryFrom(context),
                  context: context.slice(0, 240),
                },
              };
            }
            denial.el.click();
            return {
              detected: true,
              clicked: true,
              decision: 'deny',
              reason: 'default-deny-github-tool',
              candidate: {
                signature: signatureFor(context),
                index: denial.index,
                label: denial.label,
                repository: repositoryFrom(context),
                context: context.slice(0, 240),
              },
            };
          }
          node = node.parentElement || null;
        }
      }
      return { detected: false, clicked: false, decision: '', reason: '', candidate: null };
    });
  } catch (error) {
    return {
      detected: false,
      clicked: false,
      decision: 'deny',
      reason: `evaluate-failed: ${error.message}`,
      candidate: null,
    };
  }
}

async function clickPolicyControlBySignature(page, payload) {
  try {
    return await page.evaluate((policy) => {
      const normalize = (value) => String(value || '').replace(/\s+/g, ' ').trim();
      const signature = normalize(policy.signature).toLowerCase();
      const decision = normalize(policy.decision).toLowerCase();
      const controlSelector = 'button,[role="button"],a';
      const visible = (el) => {
        const style = window.getComputedStyle(el);
        const rect = el.getBoundingClientRect();
        return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
      };
      const disabled = (el) => el.hasAttribute?.('disabled') || /^true$/i.test(el.getAttribute?.('aria-disabled') || '');
      const textOf = (el) => normalize(el?.innerText || el?.textContent || '');
      const labelOf = (el) => normalize(textOf(el) || el?.getAttribute?.('aria-label') || el?.getAttribute?.('title') || '');
      const desired = decision.includes('deny')
        ? /^(deny|cancel|dismiss|not now|no thanks)$/i
        : /\b(allow|approve|continue|authorize|connect)\b/i;
      for (const root of Array.from(document.querySelectorAll('body *'))) {
        const context = textOf(root);
        if (signature && !context.toLowerCase().includes(signature)) {
          continue;
        }
        if (!signature && !/github|git\s*hub/i.test(context)) {
          continue;
        }
        const controls = Array.from(root.querySelectorAll(controlSelector)).filter((el) => visible(el) && !disabled(el));
        const match = controls.find((control) => desired.test(labelOf(control)));
        if (match) {
          const label = labelOf(match);
          match.click();
          return { clicked: true, label, reason: 'matched-policy-control' };
        }
      }
      return { clicked: false, label: '', reason: 'policy-control-not-found' };
    }, payload);
  } catch (error) {
    return { clicked: false, label: '', reason: `evaluate-failed: ${error.message}` };
  }
}

async function stopIfGenerating(page) {
  return page.evaluate(() => {
    const controls = Array.from(document.querySelectorAll('button,[role="button"],[aria-label],[title]'));
    const visible = (el) => {
      const style = window.getComputedStyle(el);
      const rect = el.getBoundingClientRect();
      return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
    };
    const disabled = (el) => el.hasAttribute?.('disabled') || /^true$/i.test(el.getAttribute?.('aria-disabled') || '');
    const label = (el) => [
      el.innerText || el.textContent || '',
      el.getAttribute?.('aria-label') || '',
      el.getAttribute?.('title') || '',
    ].join(' ').replace(/\s+/g, ' ').trim();
    for (const el of controls) {
      if (!visible(el) || disabled(el)) continue;
      const text = label(el);
      if (/\b(stop answering|stop generating|stop responding|stop thinking|stop)\b/i.test(text)) {
        el.click();
        return { clicked: true, label: text };
      }
    }
    return { clicked: false, reason: 'not-found' };
  });
}

async function finalizeTabAfterDownload(bridge, tab, envelope, reason) {
  const errors = [];
  let stopMethod = 'not-run:page-closed';
  let closed = false;
  const context = terminalCleanupContext(reason);

  if (tab.page && !tab.page.isClosed()) {
    try {
      const stop = await stopIfGenerating(tab.page);
      stopMethod = stop.clicked ? (stop.label || 'button') : `not-active:${stop.reason || 'not-found'}`;
      bridge.emit(envelope, 'generation-stopped', { method: stopMethod, phase: 'post-download' });
      bridge.bridgeLog(
        envelope,
        'generation-stopped',
        stop.clicked ? 'ok' : 'not-active',
        stop.clicked ? `stopped generation ${context}` : `generation was not active ${context}`,
        { method: stopMethod, phase: 'post-download' },
      );
    } catch (error) {
      const message = error?.message || String(error);
      errors.push(`stop:${message}`);
      bridge.bridgeLog(envelope, 'generation-stopped', 'failed', `failed to stop generation ${context}`, {
        reason: message,
      }, 'error');
    }
  }

  if (tab.page && !tab.page.isClosed()) {
    try {
      closed = await bridge.closeTabAfterReceipt(tab, envelope, reason);
    } catch (error) {
      const message = error?.message || String(error);
      errors.push(`close:${message}`);
      bridge.bridgeLog(envelope, 'tab-closed', 'failed', `failed to close tab ${context}`, {
        reason: message,
      }, 'error');
    }
  }

  return { stopMethod, closed, errors };
}

async function emitDownloadErrorAndCleanup(bridge, tab, envelope, error, details = {}) {
  const cleanup = await finalizeTabAfterDownload(bridge, tab, envelope, 'download-failed');
  const message = `failed to persist ${artifactLabelForFailure(details)}: ${error?.message || String(error)}`;
  const failedDownloadBundlePath = details.failed_download_bundle_path || details.failure_bundle_path || '';
  bridge.emit(envelope, 'error', {
    kind: 'download-failed',
    message,
    recoverable: false,
    stack: null,
    ...details,
    failed_download_bundle_path: failedDownloadBundlePath,
    cleanup_stop_method: cleanup.stopMethod,
    tab_closed: cleanup.closed,
    cleanup_errors: cleanup.errors.join(';'),
  });
  bridge.bridgeLog(envelope, 'download-failed', 'failed', message, {
    ...details,
    failed_download_bundle_path: failedDownloadBundlePath,
    cleanup_stop_method: cleanup.stopMethod,
    tab_closed: String(Boolean(cleanup.closed)),
    cleanup_errors: cleanup.errors.join(';'),
  }, 'error');
  return cleanup;
}

function artifactLabelForFailure(details = {}) {
  const kind = String(details.file_kind || details.fileKind || '').toLowerCase();
  const target = String(details.target_name || details.target_path || '').toLowerCase();
  if (kind === 'downloaded-tex' || target.endsWith('.tex')) {
    return '.tex artifact';
  }
  if (kind === 'downloaded-file') {
    return 'downloaded artifact';
  }
  if (kind === 'downloaded-archive' || target.endsWith('.tar.gz') || target.endsWith('.tgz')) {
    return 'tar.gz artifact';
  }
  return 'artifact';
}

async function emitNoTarErrorAndCleanup(bridge, tab, envelope, kind, message, details = {}) {
  const page = tab?.page;
  const pageUrl = page && !page.isClosed() && typeof page.url === 'function' ? page.url() : '';
  const noLinkBundlePath = await writeNoLinkBundle({
    bridge,
    envelope,
    tabId: envelope?.tab_id ?? tab?.browserSlot ?? tab?.tabId ?? null,
    artifactsDir: bridge?.options?.artifactsDir,
    page,
  }, {
    kind,
    message,
    pageUrl,
    details,
  });
  const cleanup = await finalizeTabAfterDownload(bridge, tab, envelope, kind);
  bridge.emit(envelope, 'error', {
    kind,
    message,
    recoverable: false,
    stack: null,
    ...details,
    failed_download_bundle_path: noLinkBundlePath,
    no_link_bundle_path: noLinkBundlePath,
    cleanup_stop_method: cleanup.stopMethod,
    tab_closed: cleanup.closed,
    cleanup_errors: cleanup.errors.join(';'),
  });
  bridge.bridgeLog(envelope, kind, 'failed', message, {
    ...details,
    failed_download_bundle_path: noLinkBundlePath,
    no_link_bundle_path: noLinkBundlePath,
    cleanup_stop_method: cleanup.stopMethod,
    tab_closed: String(Boolean(cleanup.closed)),
    cleanup_errors: cleanup.errors.join(';'),
  }, 'error');
  return cleanup;
}

async function recoverArtifactConversationDownload(bridge, tab, envelope, options = {}) {
  const page = tab?.page;
  const result = {
    downloaded: false,
    links: [],
    attempts: [],
    error: '',
  };
  if (!page || page.isClosed?.()) {
    result.error = 'source-page-closed';
    return result;
  }
  const phase = 'artifact-conversation-recovery';
  const targetName = artifactTargetName(bridge?.options || {}) || options.targetName || '';
  const state = options.state || { attempts: 0, visitedUrls: new Set() };
  const limit = Math.max(0, Math.floor(Number(bridge?.options?.artifactConversationRecoveryLimit ?? DEFAULT_ARTIFACT_CONVERSATION_RECOVERY_LIMIT)));
  const pageUrl = typeof page.url === 'function' ? page.url() : '';
  const normalizedCurrent = normalizeChatGptUrl(pageUrl);
  if (normalizedCurrent && state.visitedUrls?.add) {
    state.visitedUrls.add(normalizedCurrent);
  }

  try {
    result.links = await discoverArtifactConversationLinks(page, targetName, pageUrl);
  } catch (error) {
    result.error = error?.message || String(error);
    bridge.bridgeLog(envelope, phase, 'scan-failed', 'failed to collect artifact conversation links before no-tar', {
      source_reason: options.kind || '',
      page_url: pageUrl,
      reason: result.error,
    }, 'warn');
    return result;
  }

  if (result.links.length === 0) {
    bridge.bridgeLog(envelope, phase, 'no-links', 'no artifact conversation links found before no-tar', {
      source_reason: options.kind || '',
      page_url: pageUrl,
      target_name: targetName,
    }, 'warn');
    return result;
  }

  for (const link of result.links) {
    const attempt = {
      url: link.url || '',
      text: compact(link.text || link.aria || link.title || '', 200),
      score: link.score ?? null,
      status: '',
      reason: '',
      candidate_count: 0,
      downloaded_path: '',
    };
    result.attempts.push(attempt);

    if (!link.url) {
      attempt.status = 'skipped';
      attempt.reason = 'missing-url';
      bridge.bridgeLog(envelope, phase, 'skipped', 'skipped artifact conversation link without URL', artifactConversationAttemptLogFields(options, link, attempt), 'warn');
      continue;
    }
    if (state.visitedUrls?.has?.(link.url)) {
      attempt.status = 'skipped';
      attempt.reason = 'visited';
      bridge.bridgeLog(envelope, phase, 'skipped', 'skipped already visited artifact conversation link', artifactConversationAttemptLogFields(options, link, attempt), 'warn');
      continue;
    }
    if ((state.attempts ?? 0) >= limit) {
      attempt.status = 'skipped';
      attempt.reason = 'attempt-limit';
      bridge.bridgeLog(envelope, phase, 'skipped', 'skipped artifact conversation link because recovery attempt limit was reached', {
        ...artifactConversationAttemptLogFields(options, link, attempt),
        attempt_limit: String(limit),
        attempts_used: String(state.attempts ?? 0),
      }, 'warn');
      continue;
    }

    state.attempts = (state.attempts ?? 0) + 1;
    state.visitedUrls?.add?.(link.url);
    let recoveryPage = null;
    try {
      recoveryPage = await openArtifactConversationPage(page, link.url, bridge?.options?.browserTimeoutMs);
      attempt.status = 'opened';
      bridge.bridgeLog(envelope, phase, 'opened', 'opened artifact conversation link for artifact recovery', {
        ...artifactConversationAttemptLogFields(options, link, attempt),
        attempt: String(state.attempts),
        attempt_limit: String(limit),
      }, 'warn');

      if (typeof bridge.runDismissals === 'function') {
        await bridge.runDismissals(recoveryPage, envelope, 'artifact-conversation-recovery-dismissals');
      } else {
        await dismissPopups(recoveryPage).catch(() => undefined);
        await dismissRateLimitModal(recoveryPage).catch(() => undefined);
      }

      const discovery = await discoverTarCandidates(recoveryPage, targetName);
      const ranked = rankCandidates(discovery.candidates || [], targetName);
      attempt.candidate_count = ranked.length;
      if (ranked.length === 0) {
        attempt.status = 'no-candidate';
        bridge.bridgeLog(envelope, phase, 'no-candidate', 'artifact conversation page had no artifact download candidate', {
          ...artifactConversationAttemptLogFields(options, link, attempt),
          scanned_control_count: String(discovery.scannedControlCount ?? 0),
          assistant_roots: String(discovery.assistantRootCount ?? 0),
        }, 'warn');
        continue;
      }

      const candidate = ranked[0];
      const recovered = await downloadRecoveredArtifactConversationCandidate(bridge, tab, envelope, {
        recoveryPage,
        link,
        candidate,
        ranked,
        outputDir: options.outputDir,
        tabId: options.tabId,
      });
      attempt.status = 'downloaded';
      attempt.downloaded_path = recovered.completePayload.local_path;
      result.downloaded = true;
      result.download = recovered.completePayload;
      bridge.bridgeLog(envelope, phase, 'downloaded', 'downloaded artifact from artifact conversation page', {
        ...artifactConversationAttemptLogFields(options, link, attempt),
        local_path: recovered.completePayload.local_path,
        receipt_path: recovered.completePayload.receipt_path,
        sha256: recovered.completePayload.sha256,
        size_bytes: String(recovered.completePayload.size_bytes),
        entry_count: String(recovered.completePayload.entry_count ?? ''),
        file_kind: recovered.completePayload.file_kind,
      }, 'warn');
      return result;
    } catch (error) {
      attempt.status = attempt.status === 'opened' ? 'failed' : 'open-failed';
      attempt.reason = error?.message || String(error);
      bridge.bridgeLog(envelope, phase, attempt.status, 'artifact conversation recovery attempt failed', {
        ...artifactConversationAttemptLogFields(options, link, attempt),
        failure_bundle_path: error?.failureBundlePath || '',
      }, 'warn');
    } finally {
      if (recoveryPage && !recoveryPage.isClosed?.()) {
        try {
          await recoveryPage.close({ runBeforeUnload: false });
          bridge.bridgeLog(envelope, phase, 'closed', 'closed artifact conversation recovery page', {
            url: link.url || '',
            downloaded: String(result.downloaded),
          }, 'warn');
        } catch (error) {
          bridge.bridgeLog(envelope, phase, 'close-failed', 'failed to close artifact conversation recovery page', {
            url: link.url || '',
            reason: error?.message || String(error),
          }, 'warn');
        }
      }
    }
  }

  return result;
}

async function openArtifactConversationPage(sourcePage, url, timeoutMs = DEFAULT_BROWSER_TIMEOUT_MS) {
  const context = typeof sourcePage.context === 'function' ? sourcePage.context() : null;
  if (!context || typeof context.newPage !== 'function') {
    throw new Error('source page did not expose a browser context for artifact conversation recovery');
  }
  const page = await context.newPage();
  await page.goto(url, { waitUntil: 'domcontentloaded', timeout: Math.max(1000, timeoutMs || DEFAULT_BROWSER_TIMEOUT_MS) });
  await page.waitForLoadState?.('networkidle', { timeout: 5000 }).catch(() => undefined);
  return page;
}

async function downloadRecoveredArtifactConversationCandidate(bridge, tab, envelope, values) {
  const outputDir = values.outputDir || join(bridge.options.downloadsDir, envelope.run_id, `tab-${String(values.tabId).padStart(2, '0')}`);
  await mkdir(outputDir, { recursive: true });
  const candidate = values.candidate;
  const targetName = artifactTargetName(bridge.options);
  const candidateTargetName = downloadTargetNameForCandidate(bridge.options, candidate, targetName);
  const startedDownloadAt = timestamp();
  const targetPath = join(outputDir, candidateTargetName ? normalizeArtifactName(candidateTargetName) : normalizeArtifactName(candidate.label || candidate.download || candidate.href || 'chatgpt-output.tar.gz'));
  bridge.emit(envelope, 'tar-discovered', {
    candidates: values.ranked.slice(0, 5),
    selected_index: candidate.index,
    file_kind: candidateFileKind(candidate, candidateTargetName),
    recovered_from_artifact_conversation_url: values.link.url,
  });
  bridge.emit(envelope, 'download-started', {
    candidate_index: candidate.index,
    remote_url: candidate.href || '',
    target_path: targetPath,
    started_at: startedDownloadAt,
    recovered_from_artifact_conversation_url: values.link.url,
  });
  bridge.bridgeLog(envelope, 'download-started', 'started', 'clicking selected artifact download candidate from artifact conversation page', {
    candidate_index: String(candidate.index),
    candidate_count: String(values.ranked.length),
    candidate_score: String(candidate.score ?? ''),
    target_path: targetPath,
    target_name: candidateTargetName,
    label: compact(candidate.label || candidate.download || candidate.href || '', 160),
    artifact_conversation_url: values.link.url,
  }, 'warn');
  const file = await downloadCandidate(values.recoveryPage, candidate, outputDir, 120000, {
    bridge,
    envelope,
    tabId: values.tabId,
    artifactsDir: bridge.options.artifactsDir,
    page: values.recoveryPage,
    targetName: candidateTargetName,
  });
  const receiptPath = join(bridge.options.artifactsDir, 'receipts', envelope.run_id, `tab-${String(values.tabId).padStart(2, '0')}-download.json`);
  await mkdir(resolve(receiptPath, '..'), { recursive: true });
  const finishedDownloadAt = timestamp();
  const downloadLatencyMs = Math.max(0, Date.parse(finishedDownloadAt) - Date.parse(startedDownloadAt)) || 0;
  const completePayload = {
    sha256: file.sha256,
    size_bytes: file.sizeBytes,
    local_path: file.path,
    receipt_path: receiptPath,
    original_name: file.suggested,
    local_name: file.localName,
    file_kind: file.fileKind,
    artifact_kind: file.artifactKind,
    validation_status: file.validationStatus,
    discovery_strategy: file.persistedBy === 'materialized-from-text' ? 'materialized_from_text' : 'browser_download',
    download_url: candidate.href || null,
    entry_count: file.entryCount,
    started_at: startedDownloadAt,
    finished_at: finishedDownloadAt,
    download_latency_ms: downloadLatencyMs,
    recovered_from_artifact_conversation_url: values.link.url,
  };
  await writeFile(receiptPath, JSON.stringify(completePayload, null, 2));
  const cleanup = await finalizeTabAfterDownload(bridge, tab, envelope, 'download-complete');
  bridge.emit(envelope, 'download-complete', completePayload);
  bridge.bridgeLog(envelope, 'download-complete', 'ok', 'download receipt written after artifact conversation recovery and original tab closed', {
    sha256: completePayload.sha256,
    size_bytes: String(completePayload.size_bytes),
    entry_count: String(completePayload.entry_count),
    receipt_path: completePayload.receipt_path,
    local_path: completePayload.local_path,
    artifact_conversation_url: values.link.url,
    generation_stop_method: cleanup?.stopMethod || '',
    tab_closed: String(Boolean(cleanup?.closed)),
    cleanup_errors: (cleanup?.errors || []).join(';'),
  }, 'warn');
  if (!cleanup?.closed || cleanup.errors.length > 0) {
    bridge.bridgeLog(envelope, 'download-complete', 'cleanup-failed', 'artifact conversation download completed but original tab cleanup failed', {
      artifact_conversation_url: values.link.url,
      cleanup_stop_method: cleanup?.stopMethod || '',
      tab_closed: String(Boolean(cleanup?.closed)),
      cleanup_errors: (cleanup?.errors || []).join(';'),
    }, 'error');
  }
  return { completePayload, cleanup };
}

function artifactConversationRecoveryDetails(recovery) {
  const links = (recovery?.links || []).map((link) => ({
    url: link.url || '',
    text: compact(link.text || link.aria || link.title || '', 200),
    score: link.score ?? null,
    chapter: link.chapter || '',
    artifact_signals: link.artifactSignals || [],
  }));
  const attempts = (recovery?.attempts || []).map((attempt) => ({
    url: attempt.url || '',
    text: compact(attempt.text || '', 200),
    score: attempt.score ?? null,
    status: attempt.status || '',
    reason: compact(attempt.reason || '', 240),
    candidate_count: attempt.candidate_count ?? 0,
    downloaded_path: attempt.downloaded_path || '',
  }));
  return {
    artifact_conversation_links: links,
    artifact_conversation_attempts: attempts,
    artifact_conversation_link_count: String(links.length),
    artifact_conversation_attempt_count: String(attempts.length),
    artifact_conversation_recovery_error: recovery?.error || '',
  };
}

function artifactConversationAttemptLogFields(options, link, attempt) {
  return {
    source_reason: options.kind || '',
    url: link?.url || attempt?.url || '',
    label: compact(link?.text || link?.aria || link?.title || attempt?.text || '', 180),
    score: String(link?.score ?? attempt?.score ?? ''),
    chapter: link?.chapter || '',
    status: attempt?.status || '',
    reason: compact(attempt?.reason || '', 200),
    candidate_count: String(attempt?.candidate_count ?? 0),
  };
}

function terminalCleanupContext(reason) {
  if (reason === 'download-complete') {
    return 'after tar receipt';
  }
  if (reason === 'download-failed') {
    return 'after failed artifact download';
  }
  if (reason === 'done-no-tar') {
    return 'after assistant finished without a tar';
  }
  if (reason === 'artifact-stall-no-tar') {
    return 'after stalled artifact generation without a tar';
  }
  if (reason === 'timeout-no-tar') {
    return 'after tar wait timed out';
  }
  return `after ${reason}`;
}

async function collectKnownRunChatGptUrls(artifactsDir, currentRunId) {
  const known = new Map();
  let entries = [];
  try {
    entries = await readdir(artifactsDir, { withFileTypes: true });
  } catch {
    return known;
  }
  for (const entry of entries) {
    if (!entry.isDirectory()) {
      continue;
    }
    const eventsPath = join(artifactsDir, entry.name, 'events.ndjson');
    let data = '';
    try {
      data = await readFile(eventsPath, 'utf8');
    } catch {
      continue;
    }
    for (const line of data.split(/\r?\n/)) {
      if (!line.trim()) {
        continue;
      }
      let event;
      try {
        event = JSON.parse(line);
      } catch {
        continue;
      }
      if (event.run_id === currentRunId) {
        continue;
      }
      const pageUrl = event.fields?.page_url;
      if (!isChatGptPageUrl(pageUrl)) {
        continue;
      }
      const normalized = normalizeChatGptUrl(pageUrl);
      if (!normalized) {
        continue;
      }
      known.set(normalized, {
        runId: event.run_id || entry.name,
        tabId: event.tab_id ?? null,
        url: pageUrl,
      });
    }
  }
  return known;
}

async function recoverKnownRunPage(bridge, page, envelope, source, phase) {
  const pageUrl = page.url();
  const outputDir = join(
    bridge.options.downloadsDir,
    envelope.run_id,
    'orphan-recovery',
    sanitizePathSegment(source.runId || 'unknown-run'),
    sanitizePathSegment(source.tabId == null ? conversationIdFromChatGptUrl(pageUrl) : `tab-${source.tabId}`),
  );
  let downloaded = false;
  let closed = false;
  let localPath = '';
  try {
    await dismissPopups(page).catch(() => undefined);
    await dismissRateLimitModal(page).catch(() => undefined);
    const targetName = artifactTargetName(bridge.options);
    const discovery = await discoverTarCandidates(page, targetName);
    const ranked = rankCandidates(discovery.candidates, targetName);
    if (ranked.length > 0) {
      const candidate = ranked[0];
      const candidateTargetName = downloadTargetNameForCandidate(bridge.options, candidate, targetName);
      bridge.bridgeLog(envelope, phase, 'download-started', 'recovering download from known abandoned run tab', {
        source_run_id: source.runId || '',
        source_tab_id: source.tabId == null ? '' : String(source.tabId),
        page_url: pageUrl,
        candidate_index: String(candidate.index),
        candidate_count: String(ranked.length),
        output_dir: outputDir,
      }, 'warn');
      const file = await downloadCandidate(page, candidate, outputDir, 30000, {
        bridge,
        envelope,
        tabId: source.tabId ?? 'orphan',
        artifactsDir: bridge.options.artifactsDir,
        page,
        targetName: candidateTargetName,
      });
      localPath = file.path;
      downloaded = true;
      const receiptPath = join(
        bridge.options.artifactsDir,
        'receipts',
        envelope.run_id,
        `orphan-${sanitizePathSegment(source.runId || 'unknown-run')}-${sanitizePathSegment(source.tabId == null ? conversationIdFromChatGptUrl(pageUrl) : `tab-${source.tabId}`)}.json`,
      );
      await mkdir(resolve(receiptPath, '..'), { recursive: true });
      await writeFile(receiptPath, JSON.stringify({
        recovered_from_run_id: source.runId || null,
        recovered_from_tab_id: source.tabId,
        page_url: pageUrl,
        local_path: file.path,
        original_name: file.suggested,
        local_name: file.localName,
        file_kind: file.fileKind,
        artifact_kind: file.artifactKind,
        validation_status: file.validationStatus,
        discovery_strategy: file.persistedBy === 'materialized-from-text' ? 'materialized_from_text' : 'browser_download',
        sha256: file.sha256,
        size_bytes: file.sizeBytes,
        entry_count: file.entryCount,
        recovered_at: timestamp(),
      }, null, 2));
      bridge.bridgeLog(envelope, phase, 'downloaded', 'recovered artifact download from known abandoned run tab', {
        source_run_id: source.runId || '',
        source_tab_id: source.tabId == null ? '' : String(source.tabId),
        page_url: pageUrl,
        local_path: file.path,
        receipt_path: receiptPath,
        sha256: file.sha256,
        size_bytes: String(file.sizeBytes),
        entry_count: String(file.entryCount ?? ''),
        file_kind: file.fileKind,
      }, 'warn');
    } else {
      bridge.bridgeLog(envelope, phase, 'no-candidate', 'known abandoned run tab had no artifact candidate during recovery', {
        source_run_id: source.runId || '',
        source_tab_id: source.tabId == null ? '' : String(source.tabId),
        page_url: pageUrl,
        scanned_control_count: String(discovery.scannedControlCount ?? 0),
      }, 'warn');
    }
  } catch (error) {
    bridge.bridgeLog(envelope, phase, 'download-failed', 'failed to recover artifact from known abandoned run tab', {
      source_run_id: source.runId || '',
      source_tab_id: source.tabId == null ? '' : String(source.tabId),
      page_url: pageUrl,
      reason: error?.message || String(error),
    }, 'warn');
  } finally {
    if (!page.isClosed()) {
      try {
        const stop = await stopIfGenerating(page);
        bridge.bridgeLog(envelope, phase, stop.clicked ? 'stopped' : 'not-active', 'stopped known abandoned run tab before close', {
          source_run_id: source.runId || '',
          source_tab_id: source.tabId == null ? '' : String(source.tabId),
          page_url: pageUrl,
          method: stop.clicked ? (stop.label || 'button') : `not-active:${stop.reason || 'not-found'}`,
        }, 'warn');
      } catch (error) {
        bridge.bridgeLog(envelope, phase, 'stop-failed', 'failed to stop known abandoned run tab before close', {
          source_run_id: source.runId || '',
          source_tab_id: source.tabId == null ? '' : String(source.tabId),
          page_url: pageUrl,
          reason: error?.message || String(error),
        }, 'warn');
      }
    }
    if (!page.isClosed()) {
      try {
        await page.close({ runBeforeUnload: false });
        closed = true;
        bridge.bridgeLog(envelope, phase, 'closed', 'closed known abandoned run tab', {
          source_run_id: source.runId || '',
          source_tab_id: source.tabId == null ? '' : String(source.tabId),
          page_url: pageUrl,
          downloaded: String(downloaded),
          local_path: localPath,
        }, 'warn');
      } catch (error) {
        bridge.bridgeLog(envelope, phase, 'close-failed', 'failed to close known abandoned run tab', {
          source_run_id: source.runId || '',
          source_tab_id: source.tabId == null ? '' : String(source.tabId),
          page_url: pageUrl,
          reason: error?.message || String(error),
        }, 'error');
      }
    }
  }
  return { downloaded, closed };
}

function parseArgs(argv) {
  const parsed = {};
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (!arg.startsWith('--')) continue;
    const eq = arg.indexOf('=');
    if (eq >= 0) {
      parsed[toCamel(arg.slice(2, eq))] = arg.slice(eq + 1);
    } else {
      const key = toCamel(arg.slice(2));
      const next = argv[i + 1];
      if (next && !next.startsWith('--')) {
        parsed[key] = next;
        i += 1;
      } else {
        parsed[key] = 'true';
      }
    }
  }
  return parsed;
}

function validateEnvelope(envelope) {
  if (envelope?.v !== PROTOCOL_VERSION) {
    throw new Error(`unsupported protocol version ${envelope?.v}`);
  }
  if (!envelope.type || typeof envelope.type !== 'string') {
    throw new Error('envelope type is required');
  }
  if (!envelope.run_id || typeof envelope.run_id !== 'string') {
    throw new Error('envelope run_id is required');
  }
  if (!isSafeRunId(envelope.run_id)) {
    throw new Error('envelope run_id must be a safe path segment');
  }
  if (!envelope.ts || typeof envelope.ts !== 'string') {
    throw new Error('envelope ts is required');
  }
  if (typeof envelope.payload !== 'object' || envelope.payload === null) {
    throw new Error('envelope payload object is required');
  }
}

function requiredTabId(envelope) {
  if (!Number.isInteger(envelope.tab_id) || envelope.tab_id <= 0) {
    throw new Error(`command ${envelope.type} requires positive tab_id`);
  }
  return envelope.tab_id;
}

function requiredString(value, label) {
  if (!value || !String(value).trim()) {
    throw new Error(`${label} is required`);
  }
  return String(value);
}

function errorPayload(kind, error) {
  return {
    kind,
    message: redactSensitiveText(error?.message || String(error)),
    recoverable: false,
    stack: error?.stack ? redactSensitiveText(error.stack) : null,
  };
}

function toCamel(value) {
  return value.replace(/-([a-z])/g, (_, ch) => ch.toUpperCase());
}

function firstSetting(entries) {
  for (const [source, value] of entries) {
    if (value !== undefined && value !== null && value !== '') {
      return { source, value };
    }
  }
  return null;
}

function loadGlobalJailgunConfig() {
  const configuredPath = process.env.JAILGUN_GLOBAL_CONFIG
    ? resolvePath(process.env.JAILGUN_GLOBAL_CONFIG)
    : join(homedir(), '.jailgun', 'config.json');
  if (!existsSync(configuredPath)) {
    return {};
  }
  try {
    const parsed = JSON.parse(readFileSync(configuredPath, 'utf8'));
    const profiles = Array.isArray(parsed.profiles) ? parsed.profiles : [];
    const profilePool = profiles
      .map((profile) => {
        const id = String(profile.id || profile.name || '').trim();
        const profileDir = String(profile.profile_dir || profile.profileDir || '').trim();
        if (!id || !profileDir) return '';
        return `${id}=${profileDir}`;
      })
      .filter(Boolean)
      .join(delimiter);
    const profilePorts = profiles
      .map((profile) => {
        const id = String(profile.id || profile.name || '').trim();
        const port = Number(profile.cdp_port ?? profile.cdpPort ?? 0);
        if (!id || !Number.isInteger(port) || port <= 0 || port > 65535) return '';
        return `${id}=${port}`;
      })
      .filter(Boolean)
      .join(delimiter);
    return {
      cdpUrl: parsed.cdp_url ?? parsed.cdpUrl ?? '',
      cdpHost: parsed.cdp_host ?? parsed.cdpHost ?? '',
      cdpPort: parsed.cdp_port ?? parsed.cdpPort ?? '',
      chromeExecutable: parsed.chrome_executable ?? parsed.chromeExecutable ?? '',
      profileDir: parsed.profile_dir ?? parsed.profileDir ?? '',
      stateDir: parsed.state_dir ?? parsed.stateDir ?? '',
      profilePool,
      profilePorts,
    };
  } catch (error) {
    process.stderr.write(`[chrome-bridge] global-config: ignoring ${configuredPath}: ${error?.message || String(error)}\n`);
    return {};
  }
}

function numberFrom(value, defaultValue) {
  if (value === undefined || value === null || value === '') {
    return defaultValue;
  }
  const number = Number(value);
  if (!Number.isFinite(number)) {
    return defaultValue;
  }
  return number;
}

function booleanFrom(value, defaultValue) {
  if (value === undefined || value === null || value === '') {
    return defaultValue;
  }
  if (typeof value === 'boolean') {
    return value;
  }
  if (/^(1|true|yes|on)$/i.test(String(value))) {
    return true;
  }
  if (/^(0|false|no|off)$/i.test(String(value))) {
    return false;
  }
  return defaultValue;
}

function resolvePath(value) {
  return isAbsolute(value) ? value : resolve(process.cwd(), value);
}

function timestamp() {
  return new Date().toISOString();
}

function systemEnvelope(reason) {
  return {
    v: PROTOCOL_VERSION,
    type: 'system',
    run_id: 'unknown',
    id: `system-${Date.now()}`,
    ts: timestamp(),
    payload: { reason },
  };
}

function isChatGptPageUrl(value) {
  try {
    return new URL(value).hostname === 'chatgpt.com';
  } catch {
    return false;
  }
}

function normalizeChatGptUrl(value) {
  try {
    const url = new URL(value);
    if (url.hostname !== 'chatgpt.com') {
      return null;
    }
    return `${url.origin}${url.pathname.replace(/\/+$/, '')}`;
  } catch {
    return null;
  }
}

function conversationIdFromChatGptUrl(value) {
  try {
    const parts = new URL(value).pathname.split('/').filter(Boolean);
    return parts[parts.length - 1] || 'chatgpt-page';
  } catch {
    return 'chatgpt-page';
  }
}

function sanitizePathSegment(value) {
  return String(value || 'unknown').replace(/[^A-Za-z0-9._-]+/g, '-').slice(0, 120) || 'unknown';
}

function isSafeRunId(value) {
  const text = String(value || '');
  return text.length > 0
    && text.length <= 128
    && text !== '.'
    && text !== '..'
    && /^[A-Za-z0-9._-]+$/.test(text);
}

function compact(value, max = 240) {
  const text = String(value || '').replace(/\s+/g, ' ').trim();
  return text.length > max ? `${text.slice(0, Math.max(0, max - 3))}...` : text;
}

function normalizeBridgeLogPayload(profileFields = {}, fields = {}, message = '', status = '') {
  const normalizedFields = {};
  for (const [key, value] of Object.entries(profileFields || {})) {
    if (value !== undefined && value !== null) {
      normalizedFields[key] = redactBridgeField(key, value);
    }
  }
  for (const [key, value] of Object.entries(fields || {})) {
    if (value !== undefined && value !== null) {
      normalizedFields[key] = redactBridgeField(key, value);
    }
  }
  normalizedFields.status = status;
  return {
    redactedMessage: redactSensitiveText(message),
    normalizedFields,
  };
}

function formatBridgeStderr(envelope, phase, status, message, fields, level) {
  const tab = envelope?.tab_id ?? '-';
  const runId = envelope?.run_id ?? 'unknown';
  const fieldText = Object.entries(fields || {})
    .filter(([, value]) => value !== undefined && value !== null && value !== '')
    .map(([key, value]) => `${key}=${formatLogValue(value)}`)
    .join(' ');
  return [
    timestamp(),
    `run=${runId}`,
    `tab=${tab}`,
    `phase=${phase}`,
    `level=${String(level || 'info').toUpperCase()}`,
    `status=${String(status || '').toUpperCase()}`,
    compact(message, 300),
    fieldText,
  ].filter(Boolean).join(' | ') + '\n';
}

function formatLogValue(value) {
  const text = redactSensitiveText(value);
  if (text === '') {
    return '""';
  }
  if (/^[A-Za-z0-9._~:/@%+=,-]+$/.test(text)) {
    return text;
  }
  return JSON.stringify(text);
}

function redactBridgeField(key, value) {
  const keyText = String(key || '').toLowerCase();
  if (/\b(code|otp|token|secret|password|cookie|authorization)\b/.test(keyText)) {
    return '[redacted]';
  }
  return redactSensitiveText(value);
}

function redactSensitiveText(value) {
  return String(value ?? '')
    .replace(/\b(code|otp|token|secret|password|cookie|authorization)\s*[:=]\s*["']?[^"',\s|]+/gi, '$1=[redacted]')
    .replace(/\b\d{6,8}\b/g, '[redacted-code]');
}

function cssAttr(value) {
  return value.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
}

function normalizeTarName(value) {
  const normalized = normalizeArtifactName(value || 'chatgpt-output.tar.gz');
  if (/\.tar\.gz$/i.test(normalized)) {
    return normalized;
  }
  return `${normalized}.tar.gz`;
}

function normalizeArtifactName(value) {
  const safe = String(value || 'chatgpt-output').trim().replace(/[/\\]/g, '-');
  return safe
    .replace(/\.tar\(\d+\)\.gz$/i, '.tar.gz')
    .replace(/\.tgz$/i, '.tar.gz')
    .replace(/\.gz\.tar\.gz$/i, '.gz');
}

function artifactTargetName(options = {}) {
  return (options.downloadTargetName || options.tarTargetName || '').trim();
}

function downloadTargetNameForCandidate(options = {}, candidate = {}, targetName = artifactTargetName(options)) {
  const kind = candidateFileKind(candidate, targetName);
  if (kind === 'downloaded-archive' && isTexNameLike(targetName)) {
    return (options.tarTargetName || targetName.replace(/\.tex$/i, '.tar.gz')).trim();
  }
  return targetName;
}

function artifactWaitLabel(targetName = '') {
  if (isTexNameLike(targetName)) {
    return '.tex artifact';
  }
  if (isTarGzNameLike(targetName)) {
    return '.tar.gz artifact';
  }
  return 'artifact';
}

async function sha256File(path) {
  return createHash('sha256').update(await readFile(path)).digest('hex');
}

function firstNonEmpty(values) {
  for (const value of values) {
    if (value && value.trim()) {
      return value.trim();
    }
  }
  return null;
}

function firstMatching(values, pattern) {
  for (const value of values) {
    if (value && pattern.test(value)) {
      return value;
    }
  }
  return null;
}

function sleep(ms) {
  return new Promise((resolvePromise) => setTimeout(resolvePromise, ms));
}

async function assertDownloadCleanupSequencing() {
  const calls = [];
  const envelope = {
    v: PROTOCOL_VERSION,
    type: 'monitor-tab',
    run_id: 'run-test',
    tab_id: 3,
    ts: timestamp(),
    payload: {},
  };
  const tab = {
    page: {
      isClosed: () => false,
      evaluate: async () => {
        calls.push('stopIfGenerating');
        return { clicked: true, label: 'Stop generating' };
      },
    },
  };
  const bridge = {
    emit: (_envelope, type) => {
      calls.push(`emit:${type}`);
    },
    bridgeLog: () => undefined,
    closeTabAfterReceipt: async () => {
      calls.push('closeTabAfterReceipt');
      bridge.emit(envelope, 'tab-closed', { page_url: 'https://chatgpt.com/c/test', reason: 'download-complete' });
      tab.page = null;
      return true;
    },
  };

  const cleanup = await finalizeTabAfterDownload(bridge, tab, envelope, 'download-complete');
  const expected = [
    'stopIfGenerating',
    'emit:generation-stopped',
    'closeTabAfterReceipt',
    'emit:tab-closed',
  ];
  if (JSON.stringify(calls) !== JSON.stringify(expected)) {
    throw new Error(`download cleanup sequence failed: ${JSON.stringify(calls)}`);
  }
  if (!cleanup.closed || cleanup.stopMethod !== 'Stop generating' || cleanup.errors.length > 0) {
    throw new Error(`download cleanup result failed: ${JSON.stringify(cleanup)}`);
  }
}

async function assertNoTarCleanupSequencing(kind, message) {
  const calls = [];
  const envelope = {
    v: PROTOCOL_VERSION,
    type: 'monitor-tab',
    run_id: 'run-test',
    tab_id: 4,
    ts: timestamp(),
    payload: {},
  };
  const tab = {
    page: {
      isClosed: () => false,
      evaluate: async () => {
        calls.push('stopIfGenerating');
        return { clicked: false, reason: 'not-found' };
      },
    },
  };
  const bridge = {
    emit: (_envelope, type) => {
      calls.push(`emit:${type}`);
    },
    bridgeLog: () => undefined,
    closeTabAfterReceipt: async () => {
      calls.push('closeTabAfterReceipt');
      bridge.emit(envelope, 'tab-closed', { page_url: 'https://chatgpt.com/c/test', reason: kind });
      tab.page = null;
      return true;
    },
  };

  const cleanup = await emitNoTarErrorAndCleanup(
    bridge,
    tab,
    envelope,
    kind,
    message,
  );
  const expected = [
    'stopIfGenerating',
    'emit:generation-stopped',
    'closeTabAfterReceipt',
    'emit:tab-closed',
    'emit:error',
  ];
  if (JSON.stringify(calls) !== JSON.stringify(expected)) {
    throw new Error(`${kind} cleanup sequence failed: ${JSON.stringify(calls)}`);
  }
  if (!cleanup.closed || cleanup.stopMethod !== 'not-active:not-found' || cleanup.errors.length > 0) {
    throw new Error(`${kind} cleanup result failed: ${JSON.stringify(cleanup)}`);
  }
}

async function assertNoLinkBundleCapture(kind, message) {
  const root = await mkdtemp(join(tmpdir(), 'jailgun-no-link-bundle-'));
  try {
    const logs = [];
    const events = [];
    let evaluateCount = 0;
    const envelope = {
      v: PROTOCOL_VERSION,
      type: 'monitor-tab',
      run_id: 'run-test',
      tab_id: 7,
      ts: timestamp(),
      payload: {},
    };
    const tab = {
      browserSlot: 7,
      page: {
        isClosed: () => false,
        url: () => 'https://chatgpt.com/c/self-test',
        title: async () => 'Self Test',
        content: async () => '<html><body>Self test source</body></html>',
        evaluate: async () => {
          evaluateCount += 1;
          if (evaluateCount === 1) {
            return {
              assistantRootCount: 1,
              scannedControlCount: 2,
              candidates: [],
              lastTextLength: 18,
              lastTextPreview: 'Self test source',
              abFeedbackActive: false,
              abResponseCount: 0,
            };
          }
          return { clicked: false, reason: 'not-found' };
        },
        screenshot: async ({ path }) => {
          await writeFile(path, 'fake screenshot');
        },
      },
    };
    const bridge = {
      options: {
        artifactsDir: root,
      },
      emit: (_envelope, type, payload) => {
        events.push({ type, payload });
      },
      bridgeLog: (_envelope, phase, status, message, fields, level) => {
        logs.push({ phase, status, message, fields, level });
      },
      closeTabAfterReceipt: async () => {
        bridge.emit(envelope, 'tab-closed', { page_url: 'https://chatgpt.com/c/self-test', reason: kind });
        tab.page = null;
        return true;
      },
    };

    const cleanup = await emitNoTarErrorAndCleanup(
      bridge,
      tab,
      envelope,
      kind,
      message,
    );

    if (!cleanup.closed || cleanup.errors.length > 0) {
      throw new Error(`no-link cleanup result failed: ${JSON.stringify(cleanup)}`);
    }
    const noLinkLog = logs.find((log) => log.phase === 'no-link-bundle' && log.status === 'written');
    if (!noLinkLog?.fields?.path) {
      throw new Error(`no-link bundle path was not logged: ${JSON.stringify(logs)}`);
    }
    if (!noLinkLog.fields.path.includes('download-failures/run-test/tab-07')) {
      throw new Error(`${kind} bundle was not written under download-failures: ${JSON.stringify(noLinkLog.fields)}`);
    }
    const snapshotPath = join(noLinkLog.fields.path, 'snapshot.json');
    const snapshot = JSON.parse(await readFile(snapshotPath, 'utf8'));
    if (!snapshot.html_path || !snapshot.text_path || !snapshot.candidate_discovery_path || !snapshot.download_attempts_path) {
      throw new Error(`no-link snapshot missing fields: ${JSON.stringify(snapshot)}`);
    }
    const htmlStat = await stat(snapshot.html_path);
    const textStat = await stat(snapshot.text_path);
    const discoveryStat = await stat(snapshot.candidate_discovery_path);
    const attemptsStat = await stat(snapshot.download_attempts_path);
    const screenshotStat = await stat(snapshot.screenshot_path);
    if (!htmlStat.isFile() || !textStat.isFile() || !discoveryStat.isFile() || !attemptsStat.isFile() || !screenshotStat.isFile()) {
      throw new Error(`no-link bundle files were not written: ${JSON.stringify(snapshot)}`);
    }
    const errorEvent = events.find((event) => event.type === 'error');
    if (!errorEvent?.payload?.failed_download_bundle_path || !errorEvent?.payload?.no_link_bundle_path) {
      throw new Error(`${kind} error payload missed bundle path: ${JSON.stringify(events)}`);
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

async function assertDownloadSaveAsTempPathCopySucceeds() {
  const root = await mkdtemp(join(tmpdir(), 'jailgun-download-temp-path-'));
  try {
    const tempArchive = await createSelfTestTarGz(root);
    const logs = [];
    const page = fakeDownloadPage([
      {
        suggestedFilename: () => 'chapter-7-output.tar.gz',
        saveAs: async () => {
          const error = new Error('ENOENT: no such file or directory, copyfile');
          error.code = 'ENOENT';
          throw error;
        },
        failure: async () => null,
        path: async () => tempArchive,
      },
    ]);
    const outputDir = join(root, 'downloads');
    const file = await downloadCandidate(
      page,
      selfTestDownloadCandidate(),
      outputDir,
      1000,
      selfTestDownloadContext(logs, root),
    );
    if (file.persistedBy !== 'temp-path-copy' || !file.path.startsWith(outputDir) || file.entryCount < 1) {
      throw new Error(`download temp-path copy did not return valid file metadata: ${JSON.stringify(file)}`);
    }
    if (!logs.some((log) => log.phase === 'download-save-failed') || !logs.some((log) => log.phase === 'download-temp-path-persist' && log.status === 'copied')) {
      throw new Error(`download temp-path copy did not emit expected diagnostics: ${JSON.stringify(logs)}`);
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

async function assertDownloadSaveAsBrowserDownloadCopySucceeds() {
  const root = await mkdtemp(join(tmpdir(), 'jailgun-download-browser-dir-'));
  const oldBrowserDownloadsDir = process.env.JAILGUN_BROWSER_DOWNLOADS_DIR;
  try {
    const suggested = 'chapter-7-output.tar.gz';
    const browserDownloadsDir = join(root, 'browser-downloads');
    await mkdir(browserDownloadsDir, { recursive: true });
    process.env.JAILGUN_BROWSER_DOWNLOADS_DIR = browserDownloadsDir;
    const tempArchive = await createSelfTestTarGz(root);
    await copyFile(tempArchive, join(browserDownloadsDir, suggested));
    const missingTempPath = join(root, 'missing-playwright-temp.tar.gz');
    const logs = [];
    const page = fakeDownloadPage([
      {
        suggestedFilename: () => suggested,
        saveAs: async () => {
          const error = new Error('ENOENT: no such file or directory, copyfile');
          error.code = 'ENOENT';
          throw error;
        },
        failure: async () => null,
        path: async () => missingTempPath,
      },
    ]);
    const outputDir = join(root, 'downloads');
    const file = await downloadCandidate(
      page,
      selfTestDownloadCandidate(),
      outputDir,
      1000,
      selfTestDownloadContext(logs, root),
    );
    if (file.persistedBy !== 'browser-download-copy' || !file.path.startsWith(outputDir) || file.entryCount < 1) {
      throw new Error(`download browser-dir copy did not return valid file metadata: ${JSON.stringify(file)}`);
    }
    if (!logs.some((log) => log.phase === 'download-browser-download-persist' && log.status === 'copied')) {
      throw new Error(`download browser-dir copy did not emit expected diagnostics: ${JSON.stringify(logs)}`);
    }
  } finally {
    if (oldBrowserDownloadsDir == null) {
      delete process.env.JAILGUN_BROWSER_DOWNLOADS_DIR;
    } else {
      process.env.JAILGUN_BROWSER_DOWNLOADS_DIR = oldBrowserDownloadsDir;
    }
    await rm(root, { recursive: true, force: true });
  }
}

async function assertDirectJsonDownloadPreservesTargetName() {
  const root = await mkdtemp(join(tmpdir(), 'jailgun-json-download-'));
  try {
    const sourceJson = join(root, 'source.json');
    await writeFile(sourceJson, '{"ok":true}\n');
    const logs = [];
    const page = fakeDownloadPage([
      {
        suggestedFilename: () => 'openqg-smoke.json',
        saveAs: async (target) => {
          await copyFile(sourceJson, target);
        },
        failure: async () => null,
        path: async () => sourceJson,
      },
    ]);
    const outputDir = join(root, 'downloads');
    const file = await downloadCandidate(
      page,
      selfTestGenericFileDownloadCandidate('openqg-smoke.json'),
      outputDir,
      1000,
      {
        ...selfTestDownloadContext(logs, root),
        targetName: 'openqg-smoke.json',
      },
    );
    if (!file.path.endsWith('/openqg-smoke.json') || file.path.endsWith('.tar.gz')) {
      throw new Error(`direct json download target name was not preserved: ${JSON.stringify(file)}`);
    }
    if (file.fileKind !== 'downloaded-file' || file.artifactKind !== 'json' || file.validationStatus !== 'ok' || file.entryCount !== null) {
      throw new Error(`direct json download metadata was wrong: ${JSON.stringify(file)}`);
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

function assertTextMaterializationValidation() {
  const targetName = 'openqg-smoke.json';
  const content = selectMaterializableTextContent(targetName, {
    candidates: [
      targetName,
      `${targetName}\nExtended`,
      '```json\n{"ok":true}\n```',
    ],
  });
  if (content !== '{"ok":true}') {
    throw new Error(`json materialization did not select valid content: ${JSON.stringify(content)}`);
  }
  const bareFilename = selectMaterializableTextContent(targetName, {
    candidates: [targetName],
  });
  if (bareFilename !== '') {
    throw new Error(`bare filename should not materialize: ${JSON.stringify(bareFilename)}`);
  }
  const sandboxRepair = artifactRepairSignalFromText(
    targetName,
    `[${targetName}](sandbox:/mnt/data/${targetName}`,
    [`[${targetName}](sandbox:/mnt/data/${targetName}`],
  );
  if (!sandboxRepair.shouldRepair || sandboxRepair.reason !== 'malformed-sandbox-link') {
    throw new Error(`sandbox artifact repair signal was not detected: ${JSON.stringify(sandboxRepair)}`);
  }
  const validContentRepair = artifactRepairSignalFromText(
    targetName,
    '```json\n{"ok":true}\n```',
    ['```json\n{"ok":true}\n```'],
  );
  if (validContentRepair.shouldRepair) {
    throw new Error(`valid materializable content should not request repair: ${JSON.stringify(validContentRepair)}`);
  }
}

async function assertDownloadFailureDiagnosticsAndCleanup() {
  const root = await mkdtemp(join(tmpdir(), 'jailgun-download-failure-'));
  try {
    const logs = [];
    const events = [];
    const missingTempPath = join(root, 'missing-playwright-temp.tar.gz');
    const failingDownload = () => ({
      suggestedFilename: () => 'chapter-7-output.tar.gz',
      saveAs: async () => {
        const error = new Error('ENOENT: no such file or directory, copyfile');
        error.code = 'ENOENT';
        throw error;
      },
      failure: async () => null,
      path: async () => missingTempPath,
    });
    let downloadError = null;
    const failurePage = fakeDownloadPage([failingDownload(), failingDownload()]);
    try {
      await downloadCandidate(
        failurePage,
        selfTestDownloadCandidate(),
        join(root, 'downloads'),
        1000,
        selfTestDownloadContext(logs, root),
      );
    } catch (error) {
      downloadError = error;
    }
    if (!downloadError?.failureBundlePath) {
      throw new Error(`download failure did not include diagnostics bundle path: ${downloadError?.message || downloadError}`);
    }
    if (failurePage.clickCount() !== 1) {
      throw new Error(`failed download clicked candidate more than once: ${failurePage.clickCount()}`);
    }
    const bundleStat = await stat(downloadError.failureBundlePath);
    const htmlStat = await stat(join(downloadError.failureBundlePath, 'page.html'));
    const attempts = JSON.parse(await readFile(join(downloadError.failureBundlePath, 'download-attempts.json'), 'utf8'));
    if (!bundleStat.isDirectory() || !htmlStat.isFile() || !Array.isArray(attempts) || attempts.length !== 1) {
      throw new Error(`download failure diagnostics bundle was not written: ${downloadError.failureBundlePath}`);
    }
    for (const phase of ['download-save-failed', 'download-failure-bundle']) {
      if (!logs.some((log) => log.phase === phase)) {
        throw new Error(`download failure missed ${phase} diagnostic: ${JSON.stringify(logs)}`);
      }
    }
    if (logs.some((log) => log.phase === 'download-retry')) {
      throw new Error(`download failure should not retry candidate click: ${JSON.stringify(logs)}`);
    }

    let closeCalled = false;
    const tab = {
      browserSlot: 1,
      browserProfile: 'self-test-profile',
      browserProfileDir: join(root, 'profile'),
      browserCdpUrl: 'http://127.0.0.1:9224',
      page: {
        isClosed: () => false,
        url: () => 'https://chatgpt.com/c/self-test',
        evaluate: async () => ({ clicked: false, reason: 'not-found' }),
      },
    };
    const bridge = {
      options: {},
      emit: (_envelope, type, payload) => {
        events.push({ type, payload });
      },
      bridgeLog: (_envelope, phase, status, message, fields, level) => {
        logs.push({ phase, status, message, fields, level });
      },
      closeTabAfterReceipt: async () => {
        closeCalled = true;
        return true;
      },
    };
    const cleanup = await emitDownloadErrorAndCleanup(bridge, tab, selfTestEnvelope(), downloadError, {
      failure_bundle_path: downloadError.failureBundlePath,
      failed_download_bundle_path: downloadError.failureBundlePath,
    });
    if (!closeCalled || !cleanup.closed || cleanup.errors.length > 0) {
      throw new Error(`download failure cleanup did not close the tab: ${JSON.stringify({ closeCalled, cleanup })}`);
    }
    const errorEvent = events.find((event) => event.type === 'error');
    if (
      errorEvent?.payload?.failure_bundle_path !== downloadError.failureBundlePath
      || errorEvent.payload.failed_download_bundle_path !== downloadError.failureBundlePath
    ) {
      throw new Error(`download error payload missed diagnostics bundle paths: ${JSON.stringify(events)}`);
    }
    if (!logs.some((log) => log.phase === 'download-failed' && log.fields?.failed_download_bundle_path === downloadError.failureBundlePath)) {
      throw new Error(`download failure log missed diagnostics bundle path: ${JSON.stringify(logs)}`);
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

async function assertDirectTexDownloadFailureIsArtifactScoped() {
  const root = await mkdtemp(join(tmpdir(), 'jailgun-download-tex-failure-'));
  try {
    const logs = [];
    const events = [];
    const targetName = 'chapter-7-output.tex';
    const targetPath = join(root, 'downloads', targetName);
    const missingTempPath = join(root, 'missing-playwright-temp.tex');
    const failurePage = fakeDownloadPage([
      {
        suggestedFilename: () => targetName,
        saveAs: async (nextTargetPath) => {
          await writeFile(nextTargetPath, '');
          const error = new Error('ENOENT: no such file or directory, copyfile');
          error.code = 'ENOENT';
          throw error;
        },
        failure: async () => null,
        path: async () => missingTempPath,
      },
    ]);
    let downloadError = null;
    try {
      await downloadCandidate(
        failurePage,
        selfTestTexDownloadCandidate(targetName),
        join(root, 'downloads'),
        1000,
        {
          ...selfTestDownloadContext(logs, root),
          targetName,
          downloadsDir: join(root, 'downloads'),
        },
      );
    } catch (error) {
      downloadError = error;
    }
    if (!downloadError?.failureBundlePath) {
      throw new Error(`direct tex failure did not include diagnostics bundle path: ${downloadError?.message || downloadError}`);
    }
    if (/failed to persist tar\.gz download|download saveAs failed/.test(downloadError.message)) {
      throw new Error(`direct tex failure used stale tar/download wording: ${downloadError.message}`);
    }
    try {
      await stat(targetPath);
      throw new Error(`direct tex failure left an unusable target file behind: ${targetPath}`);
    } catch (error) {
      if (error?.code !== 'ENOENT') {
        throw error;
      }
    }
    if (!logs.some((log) => log.phase === 'download-target-path-persist' && log.status === 'removed-empty')) {
      throw new Error(`direct tex failure did not remove empty target path: ${JSON.stringify(logs)}`);
    }

    let closeCalled = false;
    const tab = {
      browserSlot: 1,
      browserProfile: 'self-test-profile',
      browserProfileDir: join(root, 'profile'),
      browserCdpUrl: 'http://127.0.0.1:9224',
      page: {
        isClosed: () => false,
        url: () => 'https://chatgpt.com/c/self-test',
        evaluate: async () => ({ clicked: false, reason: 'not-found' }),
      },
    };
    const bridge = {
      options: {},
      emit: (_envelope, type, payload) => {
        events.push({ type, payload });
      },
      bridgeLog: (_envelope, phase, status, message, fields, level) => {
        logs.push({ phase, status, message, fields, level });
      },
      closeTabAfterReceipt: async () => {
        closeCalled = true;
        return true;
      },
    };
    await emitDownloadErrorAndCleanup(bridge, tab, selfTestEnvelope(), downloadError, {
      target_name: targetName,
      target_path: targetPath,
      file_kind: 'downloaded-tex',
      failure_bundle_path: downloadError.failureBundlePath,
      failed_download_bundle_path: downloadError.failureBundlePath,
    });
    if (!closeCalled) {
      throw new Error('direct tex failure cleanup did not close the tab');
    }
    const errorEvent = events.find((event) => event.type === 'error');
    if (!errorEvent?.payload?.message?.startsWith('failed to persist .tex artifact:')) {
      throw new Error(`direct tex failure emitted wrong message: ${JSON.stringify(events)}`);
    }
    if (/failed to persist tar\.gz download|download saveAs failed/.test(errorEvent.payload.message)) {
      throw new Error(`direct tex failure payload used stale wording: ${errorEvent.payload.message}`);
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

async function createSelfTestTarGz(root) {
  const sourceDir = join(root, 'tar-source');
  const archivePath = join(root, 'self-test.tar.gz');
  await mkdir(sourceDir, { recursive: true });
  await writeFile(join(sourceDir, 'README.md'), '# self test\n');
  const result = spawnSync('tar', ['-czf', archivePath, '-C', sourceDir, '.'], { encoding: 'utf8' });
  if (result.status !== 0) {
    throw new Error(`failed to create self-test tar.gz: ${result.stderr || result.stdout}`);
  }
  return archivePath;
}

function fakeDownloadPage(downloads) {
  let waitIndex = 0;
  let clickCount = 0;
  const pageText = 'Self test download page';
  return {
    clickCount: () => clickCount,
    isClosed: () => false,
    url: () => 'https://chatgpt.com/c/self-test-download',
    title: async () => 'Self Test Download',
    content: async () => `<html><body>${pageText}</body></html>`,
    evaluate: async (fn) => {
      const previousDocument = globalThis.document;
      const previousWindow = globalThis.window;
      globalThis.document = {
        body: {
          innerText: pageText,
          textContent: pageText,
        },
        querySelectorAll: () => [],
      };
      globalThis.window = {
        getComputedStyle: () => ({ visibility: 'visible', display: 'block' }),
      };
      try {
        return fn();
      } finally {
        if (previousDocument === undefined) {
          delete globalThis.document;
        } else {
          globalThis.document = previousDocument;
        }
        if (previousWindow === undefined) {
          delete globalThis.window;
        } else {
          globalThis.window = previousWindow;
        }
      }
    },
    screenshot: async ({ path }) => {
      await writeFile(path, 'fake screenshot');
    },
    waitForEvent: async (eventName) => {
      if (eventName !== 'download') {
        throw new Error(`unexpected event wait: ${eventName}`);
      }
      const download = downloads[waitIndex];
      waitIndex += 1;
      if (!download) {
        throw new Error('no fake download available');
      }
      return download;
    },
    locator: () => ({
      nth: () => ({
        scrollIntoViewIfNeeded: async () => undefined,
        click: async () => {
          clickCount += 1;
        },
      }),
    }),
  };
}

function fakeArtifactRecoverySourcePage(links, linkedPages = []) {
  let closed = false;
  return {
    isClosed: () => closed,
    url: () => 'https://chatgpt.com/c/current-conversation',
    title: async () => 'Self Test Source',
    content: async () => '<html><body>Chapter 027 source page</body></html>',
    screenshot: async ({ path }) => {
      await writeFile(path, 'fake screenshot');
    },
    close: async () => {
      closed = true;
    },
    context: () => ({
      newPage: async () => {
        const page = linkedPages.shift();
        if (!page) {
          throw new Error('no fake artifact conversation page available');
        }
        return page;
      },
    }),
    evaluate: async () => ({ clicked: false, reason: 'not-found' }),
    __jailgunDiscoverArtifactConversationLinks: async () => links,
    __jailgunDiscoverTarCandidates: async () => ({
      assistantRootCount: 1,
      scannedControlCount: 1,
      candidates: [],
      lastTextLength: 28,
      lastTextPreview: 'Chapter 027 source page',
      abFeedbackActive: false,
      abResponseCount: 0,
      artifactConversationLinks: links,
    }),
  };
}

function fakeArtifactRecoveryConversationPage({ url, downloads = [], candidates = [] }) {
  const page = fakeDownloadPage(downloads);
  let currentUrl = url;
  let closed = false;
  return {
    ...page,
    clickCount: page.clickCount,
    isClosed: () => closed,
    url: () => currentUrl,
    goto: async (nextUrl) => {
      currentUrl = nextUrl;
    },
    waitForLoadState: async () => undefined,
    close: async () => {
      closed = true;
    },
    __jailgunDiscoverTarCandidates: async () => ({
      assistantRootCount: candidates.length > 0 ? 1 : 0,
      scannedControlCount: candidates.length,
      candidates,
      lastTextLength: candidates.length > 0 ? 42 : 18,
      lastTextPreview: candidates.length > 0 ? 'Download chapter archive' : 'No archive yet',
      abFeedbackActive: false,
      abResponseCount: 0,
      artifactConversationLinks: [],
    }),
    __jailgunDiscoverArtifactConversationLinks: async () => [],
  };
}

function fakeSuccessfulDownload(archivePath, suggested = 'chapter-027-epoch-02.tar.gz') {
  return {
    suggestedFilename: () => suggested,
    saveAs: async (targetPath) => {
      await copyFile(archivePath, targetPath);
    },
    failure: async () => null,
    path: async () => archivePath,
  };
}

function fakeArtifactRecoveryBridge(root, logs, events) {
  const bridge = {
    options: {
      downloadsDir: join(root, 'downloads-root'),
      artifactsDir: join(root, 'artifacts'),
      tarTargetName: 'chapter-027-epoch-02.tar.gz',
      browserTimeoutMs: 1000,
      artifactConversationRecoveryLimit: 3,
    },
    emit: (_envelope, type, payload) => {
      events.push({ type, payload });
    },
    bridgeLog: (_envelope, phase, status, message, fields, level) => {
      logs.push({ phase, status, message, fields, level });
    },
    runDismissals: async () => undefined,
    closeTabAfterReceipt: async (tab, envelope, reason) => {
      const pageUrl = tab.page?.url?.() || '';
      await tab.page?.close?.();
      tab.page = null;
      bridge.emit(envelope, 'tab-closed', { page_url: pageUrl, reason });
      return true;
    },
  };
  return bridge;
}

function selfTestDownloadCandidate() {
  return {
    index: 0,
    score: 500,
    label: 'Download chapter-7-output.tar.gz',
    text: 'Download chapter-7-output.tar.gz',
    href: '',
    download: 'chapter-7-output.tar.gz',
    aria: '',
    title: '',
    tag: 'a',
    role: '',
    assistantIndex: 0,
  };
}

function selfTestTexDownloadCandidate(targetName = 'chapter-7-output.tex') {
  return {
    index: 0,
    score: 700,
    label: `Download ${targetName}`,
    text: `Download ${targetName}`,
    href: '',
    download: targetName,
    aria: '',
    title: '',
    tag: 'button',
    role: '',
    assistantIndex: 0,
    fileKind: 'downloaded-tex',
  };
}

function selfTestArtifactConversationLink() {
  return {
    index: 0,
    url: 'https://chatgpt.com/c/chapter-027-artifact',
    href: 'https://chatgpt.com/c/chapter-027-artifact?model=gpt-5',
    text: 'Chapter 027 Tar.gz',
    aria: '',
    title: '',
    score: 330,
    selector: 'a[href]',
    tagName: 'a',
    conversationId: 'chapter-027-artifact',
    chapter: '27',
    targetMatched: true,
    artifactSignals: ['tar'],
  };
}

function selfTestArtifactDownloadCandidate() {
  return {
    index: 0,
    score: 700,
    label: 'Download chapter-027-epoch-02.tar.gz',
    text: 'Download chapter-027-epoch-02.tar.gz',
    href: 'blob:https://chatgpt.com/chapter-027',
    download: 'chapter-027-epoch-02.tar.gz',
    aria: '',
    title: '',
    tag: 'a',
    role: '',
    assistantIndex: 0,
  };
}

function selfTestGenericFileDownloadCandidate(targetName = 'openqg-smoke.json') {
  return {
    index: 0,
    score: 700,
    label: `Download ${targetName}`,
    text: `Download ${targetName}`,
    href: '',
    download: targetName,
    aria: '',
    title: '',
    tag: 'a',
    role: '',
    assistantIndex: 0,
    fileKind: 'downloaded-file',
  };
}

function selfTestDownloadContext(logs, root) {
  return {
    bridge: {
      bridgeLog: (_envelope, phase, status, message, fields, level) => {
        logs.push({ phase, status, message, fields, level });
      },
    },
    envelope: selfTestEnvelope(),
    tabId: 1,
    artifactsDir: join(root, 'artifacts'),
  };
}

function selfTestEnvelope() {
  return {
    v: PROTOCOL_VERSION,
    type: 'monitor-tab',
    run_id: 'run-test',
    tab_id: 1,
    ts: timestamp(),
    payload: {},
  };
}

async function assertMessageStreamRetryHardDisabled() {
  let clicked = false;
  const retryButton = {
    innerText: 'Retry',
    textContent: 'Retry',
    hasAttribute: () => false,
    getAttribute: () => '',
    getBoundingClientRect: () => ({ width: 80, height: 28 }),
    click: () => {
      clicked = true;
    },
  };
  const fakeDocument = {
    body: {
      innerText: 'Error in message stream Retry',
      textContent: 'Error in message stream Retry',
    },
    querySelectorAll: () => [retryButton],
  };
  const fakeWindow = {
    getComputedStyle: () => ({ visibility: 'visible', display: 'block' }),
  };
  const page = {
    evaluate: async (fn) => {
      const previousDocument = globalThis.document;
      const previousWindow = globalThis.window;
      globalThis.document = fakeDocument;
      globalThis.window = fakeWindow;
      try {
        return fn();
      } finally {
        if (previousDocument === undefined) {
          delete globalThis.document;
        } else {
          globalThis.document = previousDocument;
        }
        if (previousWindow === undefined) {
          delete globalThis.window;
        } else {
          globalThis.window = previousWindow;
        }
      }
    },
  };

  const status = await readGenerationStatus(page);
  if (!status.messageStreamError || !status.retryAvailable) {
    throw new Error(`message stream status detection failed: ${JSON.stringify(status)}`);
  }
  if (DEFAULT_MESSAGE_STREAM_RETRY_LIMIT !== 0 || settings.messageStreamRetryLimit !== 0) {
    throw new Error(`message stream retry limit must be hard-disabled: ${JSON.stringify({
      default: DEFAULT_MESSAGE_STREAM_RETRY_LIMIT,
      setting: settings.messageStreamRetryLimit,
    })}`);
  }
  if (DEFAULT_ARTIFACT_REPAIR_ATTEMPT_LIMIT !== 0 || settings.artifactRepairAttemptLimit !== 0) {
    throw new Error(`artifact repair attempts must be hard-disabled: ${JSON.stringify({
      default: DEFAULT_ARTIFACT_REPAIR_ATTEMPT_LIMIT,
      setting: settings.artifactRepairAttemptLimit,
    })}`);
  }
  if (clicked) {
    throw new Error('message stream Retry button was clicked even though retries are disabled');
  }
}

async function assertArtifactConversationLinkCollection() {
  const anchors = [
    fakeArtifactAnchor({
      href: 'https://chatgpt.com/c/chapter-027-artifact?model=gpt-5',
      text: 'Chapter 027 Tar.gz',
    }),
    fakeArtifactAnchor({
      href: 'https://chatgpt.com/c/chapter-028-artifact',
      text: 'Chapter 028 Tar.gz',
    }),
    fakeArtifactAnchor({
      href: 'https://chatgpt.com/c/chapter-027-review',
      text: 'Chapter 027 Editorial Review',
    }),
    fakeArtifactAnchor({
      href: 'https://chatgpt.com/c/current-conversation?locale=en-US',
      text: 'Chapter 027 Artifact',
    }),
    fakeArtifactAnchor({
      href: 'https://chatgpt.com/c/chapter-027-upload',
      text: 'Chapter 027 Tar.gz',
      uploadChip: true,
    }),
  ];
  const page = fakeArtifactDomPage(anchors, 'https://chatgpt.com/c/current-conversation');
  const links = await discoverArtifactConversationLinks(page, 'chapter-027-epoch-02.tar.gz', page.url());
  if (links.length !== 1 || links[0].url !== 'https://chatgpt.com/c/chapter-027-artifact') {
    throw new Error(`artifact conversation link collection failed: ${JSON.stringify(links)}`);
  }
  const tarDiscovery = await discoverTarCandidates(page);
  if (tarDiscovery.candidates.length !== 0) {
    throw new Error(`artifact conversation link was discovered as a direct tar candidate: ${JSON.stringify(tarDiscovery.candidates)}`);
  }
}

function fakeArtifactAnchor({ href, text, aria = '', title = '', uploadChip = false }) {
  return {
    href,
    innerText: text,
    textContent: text,
    tagName: 'A',
    hasAttribute: () => false,
    getBoundingClientRect: () => ({ width: 120, height: 24 }),
    getAttribute: (name) => ({
      href,
      'aria-label': aria,
      title,
    })[name] || '',
    closest: (selector) => (uploadChip && /\[data-testid\*="upload-chip"\]/.test(selector) ? {} : null),
  };
}

function fakeArtifactDomPage(anchors, currentUrl) {
  return {
    url: () => currentUrl,
    evaluate: async (fn, arg) => {
      const previousDocument = globalThis.document;
      const previousWindow = globalThis.window;
      globalThis.document = {
        location: { href: currentUrl },
        body: {
          innerText: anchors.map((anchor) => anchor.textContent).join(' '),
          textContent: anchors.map((anchor) => anchor.textContent).join(' '),
        },
        querySelectorAll: (selector) => {
          if (selector === 'a[href]' || selector === 'a,button,[role="button"],[download],[href]') {
            return anchors;
          }
          return [];
        },
      };
      globalThis.window = {
        getComputedStyle: () => ({ visibility: 'visible', display: 'block' }),
      };
      try {
        return fn(arg);
      } finally {
        if (previousDocument === undefined) {
          delete globalThis.document;
        } else {
          globalThis.document = previousDocument;
        }
        if (previousWindow === undefined) {
          delete globalThis.window;
        } else {
          globalThis.window = previousWindow;
        }
      }
    },
  };
}

async function assertArtifactConversationRecoveryDownloadsFromLinkedPage() {
  const root = await mkdtemp(join(tmpdir(), 'jailgun-artifact-recovery-download-'));
  try {
    const logs = [];
    const events = [];
    const archivePath = await createSelfTestTarGz(root);
    const linkedPage = fakeArtifactRecoveryConversationPage({
      url: 'https://chatgpt.com/c/chapter-027-artifact',
      downloads: [fakeSuccessfulDownload(archivePath)],
      candidates: [selfTestArtifactDownloadCandidate()],
    });
    const sourcePage = fakeArtifactRecoverySourcePage([selfTestArtifactConversationLink()], [linkedPage]);
    const tab = { browserSlot: 1, page: sourcePage };
    const bridge = fakeArtifactRecoveryBridge(root, logs, events);
    const recovery = await recoverArtifactConversationDownload(bridge, tab, selfTestEnvelope(), {
      kind: 'done-no-tar',
      message: 'assistant finished but no tar.gz download candidate was found',
      outputDir: join(root, 'downloads'),
      tabId: 1,
      targetName: bridge.options.tarTargetName,
      state: { attempts: 0, visitedUrls: new Set(['https://chatgpt.com/c/current-conversation']) },
    });

    if (!recovery.downloaded || !recovery.download?.local_path) {
      throw new Error(`artifact conversation recovery did not download: ${JSON.stringify(recovery)}`);
    }
    if (linkedPage.clickCount() !== 1) {
      throw new Error(`artifact conversation recovery clicked the tar candidate ${linkedPage.clickCount()} times`);
    }
    if (tab.page !== null || !events.some((event) => event.type === 'download-complete')) {
      throw new Error(`artifact conversation recovery did not close original tab and emit completion: ${JSON.stringify({ tabPage: tab.page, events })}`);
    }
    if (!logs.some((log) => log.phase === 'artifact-conversation-recovery' && log.status === 'downloaded')) {
      throw new Error(`artifact conversation recovery did not log downloaded result: ${JSON.stringify(logs)}`);
    }
    const receiptStat = await stat(recovery.download.receipt_path);
    if (!receiptStat.isFile()) {
      throw new Error(`artifact conversation recovery receipt was not written: ${recovery.download.receipt_path}`);
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

async function assertArtifactConversationRecoveryNoCandidateDiagnostics() {
  const root = await mkdtemp(join(tmpdir(), 'jailgun-artifact-recovery-no-candidate-'));
  try {
    const logs = [];
    const events = [];
    const linkedPage = fakeArtifactRecoveryConversationPage({
      url: 'https://chatgpt.com/c/chapter-027-artifact',
      downloads: [],
      candidates: [],
    });
    const sourcePage = fakeArtifactRecoverySourcePage([selfTestArtifactConversationLink()], [linkedPage]);
    const tab = { browserSlot: 2, page: sourcePage };
    const bridge = fakeArtifactRecoveryBridge(root, logs, events);
    const recovery = await recoverArtifactConversationDownload(bridge, tab, selfTestEnvelope(), {
      kind: 'done-no-tar',
      message: 'assistant finished but no tar.gz download candidate was found',
      outputDir: join(root, 'downloads'),
      tabId: 2,
      targetName: bridge.options.tarTargetName,
      state: { attempts: 0, visitedUrls: new Set(['https://chatgpt.com/c/current-conversation']) },
    });

    if (recovery.downloaded) {
      throw new Error(`artifact conversation recovery unexpectedly downloaded: ${JSON.stringify(recovery)}`);
    }
    if (linkedPage.clickCount() !== 0) {
      throw new Error(`artifact conversation no-candidate recovery clicked a link as a download: ${linkedPage.clickCount()}`);
    }
    if (!logs.some((log) => log.phase === 'artifact-conversation-recovery' && log.status === 'no-candidate')) {
      throw new Error(`artifact conversation recovery missed no-candidate log: ${JSON.stringify(logs)}`);
    }

    await emitNoTarErrorAndCleanup(
      bridge,
      tab,
      selfTestEnvelope(),
      'done-no-tar',
      'assistant finished but no tar.gz download candidate was found',
      artifactConversationRecoveryDetails(recovery),
    );
    const noLinkLog = logs.find((log) => log.phase === 'no-link-bundle' && log.status === 'written');
    if (!noLinkLog?.fields?.path) {
      throw new Error(`artifact conversation no-tar diagnostics bundle was not written: ${JSON.stringify(logs)}`);
    }
    const snapshot = JSON.parse(await readFile(join(noLinkLog.fields.path, 'snapshot.json'), 'utf8'));
    if (snapshot.details?.artifact_conversation_links?.[0]?.url !== 'https://chatgpt.com/c/chapter-027-artifact') {
      throw new Error(`artifact conversation links missing from no-link diagnostics: ${JSON.stringify(snapshot.details)}`);
    }
    if (!events.some((event) => event.type === 'error' && event.payload?.kind === 'done-no-tar')) {
      throw new Error(`artifact conversation no-candidate path did not continue to no-tar error: ${JSON.stringify(events)}`);
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

async function assertKnownRunUrlCollection() {
  const root = await mkdtemp(join(tmpdir(), 'jailgun-known-run-'));
  try {
    await mkdir(join(root, 'run-old'), { recursive: true });
    await mkdir(join(root, 'run-current'), { recursive: true });
    await writeFile(join(root, 'run-old', 'events.ndjson'), [
      JSON.stringify({
        run_id: 'run-old',
        tab_id: 4,
        fields: { page_url: 'https://chatgpt.com/c/old-conversation/' },
      }),
      JSON.stringify({
        run_id: 'run-old',
        tab_id: 5,
        fields: { page_url: 'https://example.invalid/c/not-chatgpt' },
      }),
    ].join('\n'));
    await writeFile(join(root, 'run-current', 'events.ndjson'), JSON.stringify({
      run_id: 'run-current',
      tab_id: 1,
      fields: { page_url: 'https://chatgpt.com/c/current-conversation' },
    }));
    const known = await collectKnownRunChatGptUrls(root, 'run-current');
    if (!known.has('https://chatgpt.com/c/old-conversation')) {
      throw new Error(`known run URL collection missed prior ChatGPT URL: ${JSON.stringify([...known.keys()])}`);
    }
    if (known.has('https://chatgpt.com/c/current-conversation')) {
      throw new Error('known run URL collection included current run URL');
    }
    if ([...known.keys()].some((url) => url.includes('example.invalid'))) {
      throw new Error(`known run URL collection included non-ChatGPT URL: ${JSON.stringify([...known.keys()])}`);
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

async function assertBrowserProfilePoolPlanning() {
  const root = await mkdtemp(join(tmpdir(), 'jailgun-profile-pool-'));
  try {
    const pool = buildBrowserProfilePool({
      profilePoolValue: [
        `writer=${join(root, 'google-a')}`,
        `reviewer=${join(root, 'google-b')}`,
      ].join(delimiter),
      profilePoolSource: 'self-test',
      profilePortsValue: [
        'writer=9224',
        'reviewer=9301',
      ].join(delimiter),
      defaultProfileDir: join(root, 'default-profile'),
      defaultStateDir: join(root, 'state'),
      baseCdpUrl: 'http://127.0.0.1:9224',
      cdpEndpointSource: 'self-test',
    });
    if (pool.length !== 2) {
      throw new Error(`profile pool should contain two slots: ${JSON.stringify(pool)}`);
    }
    if (pool[0].profileName !== 'writer' || pool[1].profileName !== 'reviewer') {
      throw new Error(`profile names were not preserved: ${JSON.stringify(pool)}`);
    }
    if (pool[0].cdpUrl !== 'http://127.0.0.1:9224' || pool[1].cdpUrl !== 'http://127.0.0.1:9301') {
      throw new Error(`profile pool did not honor explicit CDP ports: ${JSON.stringify(pool)}`);
    }
    if (!pool[1].stateDir.endsWith(join('state', 'profiles', 'reviewer'))) {
      throw new Error(`profile pool state dir did not isolate by profile: ${pool[1].stateDir}`);
    }
    const first = profilePoolSlotForTab(pool, 1);
    const second = profilePoolSlotForTab(pool, 2);
    const third = profilePoolSlotForTab(pool, 3);
    if (first.profileName !== 'writer' || second.profileName !== 'reviewer' || third.profileName !== 'writer') {
      throw new Error(`profile slot round-robin failed: ${JSON.stringify([first, second, third])}`);
    }
    const exact = findProfileSlotByDir(pool, join(root, 'google-b'));
    if (exact.profileName !== 'reviewer') {
      throw new Error(`open-tab profile_dir did not select exact profile: ${JSON.stringify(exact)}`);
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

async function assertManagedBrowserTerminationSequence() {
  const closeCalls = [];
  const closeResult = await requestManagedBrowserClose({
    browser: {
      newBrowserCDPSession: async () => ({
        send: async (method) => {
          closeCalls.push(method);
        },
      }),
    },
  }, 100);
  if (JSON.stringify(closeCalls) !== JSON.stringify(['Browser.close'])) {
    throw new Error(`managed browser CDP close command failed: ${JSON.stringify(closeCalls)}`);
  }
  if (!closeResult.sent || closeResult.status !== 'sent') {
    throw new Error(`managed browser CDP close result failed: ${JSON.stringify(closeResult)}`);
  }

  const closeObserved = await requestManagedBrowserClose({
    browser: {
      newBrowserCDPSession: async () => ({
        send: async () => {
          throw new Error('Protocol error (Browser.close): Target closed');
        },
      }),
    },
  }, 100);
  if (!closeObserved.sent || closeObserved.status !== 'sent-close-observed') {
    throw new Error(`managed browser CDP close observed result failed: ${JSON.stringify(closeObserved)}`);
  }

  const forcedCalls = [];
  let forcedAlive = true;
  let forcedPortOpen = true;
  const forced = await terminateManagedBrowserProcess({
    pid: 4242,
    cdpUrl: 'http://127.0.0.1:9224',
  }, {
    timeoutMs: 0,
    isProcessAlive: () => forcedAlive,
    isPortOpen: async () => forcedPortOpen,
    killProcess: (pid, signal) => {
      forcedCalls.push(`${signal}:${pid}`);
      if (signal === 'SIGKILL') {
        forcedAlive = false;
        forcedPortOpen = false;
      }
    },
    sleep: async () => undefined,
  });
  if (JSON.stringify(forcedCalls) !== JSON.stringify(['SIGTERM:4242', 'SIGKILL:4242'])) {
    throw new Error(`managed browser forced termination sequence failed: ${JSON.stringify(forcedCalls)}`);
  }
  if (forced.status !== 'ok' || !forced.sigterm_sent || !forced.sigkill_sent || !forced.port_closed) {
    throw new Error(`managed browser forced termination result failed: ${JSON.stringify(forced)}`);
  }

  const gracefulCalls = [];
  let gracefulAlive = true;
  let gracefulPortOpen = true;
  const graceful = await terminateManagedBrowserProcess({
    pid: 4243,
    cdpUrl: 'http://127.0.0.1:9225',
  }, {
    timeoutMs: 100,
    isProcessAlive: () => gracefulAlive,
    isPortOpen: async () => gracefulPortOpen,
    killProcess: (pid, signal) => {
      gracefulCalls.push(`${signal}:${pid}`);
      if (signal === 'SIGTERM') {
        gracefulAlive = false;
        gracefulPortOpen = false;
      }
    },
    sleep: async () => undefined,
  });
  if (JSON.stringify(gracefulCalls) !== JSON.stringify(['SIGTERM:4243'])) {
    throw new Error(`managed browser graceful termination sequence failed: ${JSON.stringify(gracefulCalls)}`);
  }
  if (graceful.status !== 'ok' || !graceful.sigterm_sent || graceful.sigkill_sent || !graceful.port_closed) {
    throw new Error(`managed browser graceful termination result failed: ${JSON.stringify(graceful)}`);
  }

  const skipped = await terminateManagedBrowserProcess({ pid: null, cdpUrl: 'http://127.0.0.1:9226' });
  if (skipped.status !== 'skipped') {
    throw new Error(`managed browser missing pid should be skipped: ${JSON.stringify(skipped)}`);
  }
}

function assertTransientNavigationErrorClassification() {
  if (!isTransientNavigationError(new Error('page.evaluate: Execution context was destroyed, most likely because of a navigation'))) {
    throw new Error('navigation-destroyed Playwright error should be retryable');
  }
  if (isTransientNavigationError(new Error('Target page, context or browser has been closed'))) {
    throw new Error('closed target errors should not be classified as transient navigation');
  }
}

function assertEnvelopeRunIdValidation() {
  for (const runId of ['../outside', 'bad/run', 'bad\\run', '.', '..', '']) {
    let rejected = false;
    try {
      validateEnvelope({
        v: 1,
        type: 'hello',
        run_id: runId,
        ts: timestamp(),
        payload: {},
      });
    } catch (error) {
      if (!String(error?.message || error).includes('run_id')) {
        throw error;
      }
      rejected = true;
    }
    if (!rejected) {
      throw new Error(`unsafe run_id accepted: ${JSON.stringify(runId)}`);
    }
  }
}

function assertErrorPayloadRedaction() {
  const error = new Error('code=123456 token=abc123');
  error.stack = 'Error: password=hunter2 7654321';
  const payload = errorPayload('self-test', error);
  const serialized = JSON.stringify(payload);
  for (const leaked of ['123456', 'abc123', 'hunter2', '7654321']) {
    if (serialized.includes(leaked)) {
      throw new Error(`error payload leaked sensitive text: ${serialized}`);
    }
  }
  if (!serialized.includes('[redacted')) {
    throw new Error(`error payload did not include redaction markers: ${serialized}`);
  }
}

function assertBridgeLogProfileFieldRedaction() {
  const payload = normalizeBridgeLogPayload(
    {
      browser_profile: 'acct-token=abc123',
      browser_profile_dir: '/tmp/profile-password=hunter2',
      cdp_url: 'http://127.0.0.1:9224/?token=secret123',
      browser_slot: 1,
    },
    {},
    'code=123456',
    'ok',
  );
  const serialized = JSON.stringify(payload);
  for (const leaked of ['abc123', 'hunter2', 'secret123', '123456']) {
    if (serialized.includes(leaked)) {
      throw new Error(`bridge log profile field leaked sensitive text: ${serialized}`);
    }
  }
}

function assertManagedChromeLaunchPlanning() {
  const noDisplay = managedDisplayPlan({});
  if (!noDisplay.needsXvfb || noDisplay.display !== '') {
    throw new Error(`missing DISPLAY should plan Xvfb launch: ${JSON.stringify(noDisplay)}`);
  }
  const withDisplay = managedDisplayPlan({ DISPLAY: ':77' });
  if (withDisplay.needsXvfb || withDisplay.display !== ':77') {
    throw new Error(`existing DISPLAY should skip Xvfb launch: ${JSON.stringify(withDisplay)}`);
  }
  const launchArgs = buildManagedChromeLaunchArgs(
    { profileDir: '/tmp/jailgun-profile' },
    { port: 9224, hostname: '127.0.0.1' },
  );
  if (launchArgs.includes('--headless=new') || !launchArgs.includes('--new-window')) {
    throw new Error(`managed Chrome launch args must stay headed: ${JSON.stringify(launchArgs)}`);
  }
}

async function assertProfileLockCleanup() {
  const root = await mkdtemp(join(tmpdir(), 'jailgun-lock-cleanup-'));
  try {
    const profileDir = join(root, 'profile');
    await mkdir(profileDir, { recursive: true });
    await Promise.all([
      writeFile(join(profileDir, 'SingletonLock'), 'lock'),
      writeFile(join(profileDir, 'SingletonCookie'), 'cookie'),
      writeFile(join(profileDir, 'SingletonSocket'), 'socket'),
      writeFile(join(profileDir, 'Lockfile'), 'lockfile'),
    ]);
    if (detectProfileLockArtifacts(profileDir).length !== 4) {
      throw new Error('profile lock detection should find all stale artifacts');
    }
    await clearProfileLockArtifacts(profileDir);
    if (detectProfileLockArtifacts(profileDir).length !== 0) {
      throw new Error('profile lock cleanup did not remove stale artifacts');
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

async function assertSessionExpiredFailFast() {
  const bridge = new ChromeBridge({ profilePool: [] });
  const events = [];
  bridge.bridgeLog = () => undefined;
  bridge.emit = (_envelope, type, payload) => {
    events.push({ type, payload });
  };
  const fakeDialog = {
    ownerDocument: {
      defaultView: {
        getComputedStyle: () => ({ visibility: 'visible', display: 'block' }),
      },
    },
    innerText: 'Your session expired. Sign in again.',
    textContent: 'Your session expired. Sign in again.',
    getAttribute: () => '',
    getBoundingClientRect: () => ({ width: 320, height: 180 }),
    querySelectorAll: () => [],
  };
  const page = {
    url: () => 'https://chatgpt.com/',
    evaluate: async (fn) => {
      const previousDocument = globalThis.document;
      const previousWindow = globalThis.window;
      globalThis.document = {
        body: {
          innerText: 'Your session expired. Sign in again.',
          textContent: 'Your session expired. Sign in again.',
        },
        querySelectorAll: () => [fakeDialog],
      };
      globalThis.window = {
        getComputedStyle: () => ({ visibility: 'visible', display: 'block' }),
      };
      try {
        return fn();
      } finally {
        if (previousDocument === undefined) {
          delete globalThis.document;
        } else {
          globalThis.document = previousDocument;
        }
        if (previousWindow === undefined) {
          delete globalThis.window;
        } else {
          globalThis.window = previousWindow;
        }
      }
    },
  };
  let failed = false;
  try {
    await bridge.runDismissals(page, {
      run_id: 'run-test',
      type: 'monitor-tab',
      tab_id: 1,
      ts: timestamp(),
      payload: {},
    }, 'monitor-dismissals');
  } catch (error) {
    failed = /session expired prompt detected/i.test(error.message);
  }
  if (!failed) {
    throw new Error('session-expired popup should fail fast');
  }
  if (!events.some((event) => event.type === 'session-expired')) {
    throw new Error(`session-expired event was not emitted: ${JSON.stringify(events)}`);
  }
}

async function assertEmailCodeSelectionAndManualAuthFallbacks() {
  for (const text of [
    'password',
    'captcha',
    'passkey',
    'sms',
  ]) {
    const action = manualAuthActionFromText(text.toLowerCase());
    if (!action || action.action !== 'manual-browser-required') {
      throw new Error(`manual auth handling failed for text: ${text}`);
    }
  }

  let clickedIndex = -1;
  const elements = [
    {
      innerText: 'Send email verification code',
      textContent: 'Send email verification code',
      getAttribute: (name) => ({
        'aria-label': '',
        title: '',
        value: '',
      })[name] || '',
      getBoundingClientRect: () => ({ width: 120, height: 28 }),
    },
    {
      innerText: 'Send SMS verification code',
      textContent: 'Send SMS verification code',
      getAttribute: (name) => ({
        'aria-label': '',
        title: '',
        value: '',
      })[name] || '',
      getBoundingClientRect: () => ({ width: 120, height: 28 }),
    },
  ];
  const page = {
    locator: () => ({
      evaluateAll: async (fn) => {
        const previousWindow = globalThis.window;
        globalThis.window = {
          getComputedStyle: () => ({ visibility: 'visible', display: 'block' }),
        };
        try {
          return fn(elements);
        } finally {
          if (previousWindow === undefined) {
            delete globalThis.window;
          } else {
            globalThis.window = previousWindow;
          }
        }
      },
      nth: (index) => ({
        click: async () => {
          clickedIndex = index;
        },
      }),
    }),
    waitForLoadState: async () => undefined,
  };
  const selected = await selectEmailCodeControl(page);
  if (!selected.clicked || clickedIndex !== 0 || !/email/i.test(selected.destinationHint || '')) {
    throw new Error(`email code selection did not choose the email control: ${JSON.stringify({ selected, clickedIndex })}`);
  }
}

async function assertComposerWaitAndAuthClassification() {
  let composerChecks = 0;
  const delayedComposer = {
    url: () => 'https://chatgpt.com/',
    locator: (selector) => fakeComposerLocator(selector, () => {
      if (selector === '#prompt-textarea') {
        composerChecks += 1;
        return composerChecks >= 2;
      }
      return false;
    }, ''),
  };
  const composer = await waitForChatComposer(delayedComposer, 1500, {
    dismiss: async () => undefined,
    log: () => undefined,
  });
  if (!composer || composerChecks < 2) {
    throw new Error('delayed composer should be returned before timeout');
  }

  const states = [];
  const loginPage = {
    url: () => 'https://chatgpt.com/auth/login',
    locator: (selector) => fakeComposerLocator(selector, () => (
      selector === 'button:has-text("Log in")'
    ), 'Log in to continue'),
  };
  let authRequired = false;
  try {
    await waitForChatComposer(loginPage, 1000, {
      dismiss: async () => undefined,
      authState: (state) => states.push(state.state),
      log: () => undefined,
    });
  } catch (error) {
    authRequired = String(error?.message || error).startsWith('auth-required:');
  }
  if (!authRequired || !states.includes('auth-required')) {
    throw new Error(`auth-required composer wait classification failed: ${JSON.stringify(states)}`);
  }
}

function fakeComposerLocator(selector, visibleForSelector, bodyText) {
  const visible = () => Boolean(visibleForSelector(selector));
  return {
    first() {
      return this;
    },
    count: async () => visible() ? 1 : 0,
    isVisible: async () => visible(),
    innerText: async () => selector === 'body' ? bodyText : '',
  };
}

async function assertKeepAliveCleanup() {
  const bridge = new ChromeBridge({ profilePool: [] });
  const calls = [];
  bridge.bridgeLog = () => undefined;
  const page = {
    isClosed: () => false,
    evaluate: async () => {
      calls.push('ping');
    },
  };
  await bridge.pingKeepAlive('tab:1', page, systemEnvelope('keepalive-test'), 'tab-keep-alive');
  if (JSON.stringify(calls) !== JSON.stringify(['ping'])) {
    throw new Error(`keep-alive ping failed: ${JSON.stringify(calls)}`);
  }
  bridge.startKeepAlive('tab:1', page, systemEnvelope('keepalive-test'), 'tab-keep-alive', 5);
  if (!bridge.keepAliveTimers.has('tab:1')) {
    throw new Error('keep-alive timer was not registered');
  }
  bridge.clearKeepAlive('tab:1');
  if (bridge.keepAliveTimers.has('tab:1')) {
    throw new Error('keep-alive timer was not cleared');
  }
  bridge.startKeepAlive('tab:2', page, systemEnvelope('keepalive-test'), 'tab-keep-alive', 5);
  bridge.startKeepAlive('tab:3', page, systemEnvelope('keepalive-test'), 'tab-keep-alive', 5);
  bridge.clearAllKeepAlives();
  if (bridge.keepAliveTimers.size !== 0) {
    throw new Error('clearAllKeepAlives did not clear all timers');
  }
}

async function runSelfTest() {
  const name = normalizeTarName('jekko-fixes.tgz');
  if (name !== 'jekko-fixes.tar.gz') {
    throw new Error(`normalizeTarName failed: ${name}`);
  }
  const jsonName = normalizeArtifactName('openqg-smoke.json');
  if (jsonName !== 'openqg-smoke.json') {
    throw new Error(`normalizeArtifactName should preserve json target names: ${jsonName}`);
  }
  const ranked = rankCandidates([
    { score: 1, text: 'other.tar.gz', href: '', download: '', aria: '', title: '', assistantIndex: 0 },
    { score: 1, text: 'Download jekko-fixes.tar.gz', href: '', download: '', aria: '', title: '', assistantIndex: 0 },
  ], 'jekko-fixes.tar.gz');
  if (!ranked[0].text.includes('jekko-fixes')) {
    throw new Error('target tar ranking failed');
  }
  const jsonRanked = rankCandidates([
    { score: 1, text: 'Download openqg-smoke.json', href: '', download: 'openqg-smoke.json', aria: '', title: '', assistantIndex: 0 },
  ], 'openqg-smoke.json');
  if (jsonRanked.length !== 1 || candidateFileKind(jsonRanked[0], 'openqg-smoke.json') !== 'downloaded-file') {
    throw new Error(`target json ranking failed: ${JSON.stringify(jsonRanked)}`);
  }
  const wrongGeneric = rankCandidates([
    { score: 1, text: 'Download wrong.qg', href: '', download: '', aria: '', title: '', assistantIndex: 0 },
  ], 'openqg-smoke.qg');
  if (wrongGeneric.length !== 0) {
    throw new Error(`wrong arbitrary-extension candidate was not filtered: ${JSON.stringify(wrongGeneric)}`);
  }
  const filteredHistoryTarLabel = rankCandidates([
    {
      score: 270,
      text: 'Missing .tar(289).gz Archive',
      href: 'https://chatgpt.com/c/6a224b5f-25f0-83e8-8556-b960941c7551',
      download: '',
      aria: 'Missing .tar(289).gz Archive, unread',
      title: '',
      tag: 'a',
      assistantIndex: null,
    },
    {
      score: 220,
      text: '',
      href: '',
      download: '',
      aria: 'Open conversation options for Missing .tar(289).gz Archive',
      title: '',
      tag: 'button',
      assistantIndex: null,
    },
  ], '');
  if (filteredHistoryTarLabel.length !== 0) {
    throw new Error(`chat history tar label candidate was not filtered: ${JSON.stringify(filteredHistoryTarLabel)}`);
  }
  const legacyFallback = planCdpEndpointRecovery(
    parseCdpEndpoint('http://127.0.0.1:922'),
    { status: 'closed', reason: 'connection refused' },
    [
      {
        endpoint: parseCdpEndpoint('http://127.0.0.1:9224'),
        probe: { status: 'closed', reason: 'connection refused' },
      },
    ],
  );
  if (legacyFallback.endpoint.origin !== 'http://127.0.0.1:9224' || !legacyFallback.recovery) {
    throw new Error(`local CDP port 922 redirect failed: ${JSON.stringify(legacyFallback)}`);
  }
  const legacyBlockedDefault = planCdpEndpointRecovery(
    parseCdpEndpoint('http://localhost:922'),
    { status: 'open-non-cdp', reason: 'Unexpected token < in JSON' },
    [
      {
        endpoint: parseCdpEndpoint('http://127.0.0.1:9224'),
        probe: { status: 'open-non-cdp', reason: 'not Chrome CDP' },
      },
      {
        endpoint: parseCdpEndpoint('http://127.0.0.1:9225'),
        probe: { status: 'closed', reason: 'connection refused' },
      },
    ],
  );
  if (legacyBlockedDefault.endpoint.origin !== 'http://127.0.0.1:9225' || legacyBlockedDefault.recovery.blocked_cdp_urls[0] !== 'http://127.0.0.1:9224') {
    throw new Error(`managed CDP port scan failed: ${JSON.stringify(legacyBlockedDefault)}`);
  }
  const allManagedBlocked = planCdpEndpointRecovery(
    parseCdpEndpoint('http://127.0.0.1:922'),
    { status: 'closed', reason: 'connection refused' },
    managedCdpEndpoints().map((endpoint) => ({
      endpoint,
      probe: { status: 'open-non-cdp', reason: 'not Chrome CDP' },
    })),
  );
  if (!allManagedBlocked.fatal || allManagedBlocked.fatal.checked_port !== 9224 || !allManagedBlocked.fatal.next_action.includes('lsof -nP -iTCP:9224')) {
    throw new Error(`blocked managed CDP ports should return a clear fatal plan: ${JSON.stringify(allManagedBlocked)}`);
  }
  const validLegacy = planCdpEndpointRecovery(
    parseCdpEndpoint('http://localhost:922'),
    { status: 'cdp', reason: 'ok' },
  );
  if (validLegacy.endpoint.origin !== 'http://localhost:922' || validLegacy.recovery) {
    throw new Error(`valid local CDP port 922 should stay selected: ${JSON.stringify(validLegacy)}`);
  }
  const remoteLegacy = planCdpEndpointRecovery(
    parseCdpEndpoint('http://cdp.example.test:922'),
    { status: 'closed', reason: 'unreachable' },
  );
  if (remoteLegacy.endpoint.origin !== 'http://cdp.example.test:922' || remoteLegacy.recovery) {
    throw new Error(`remote CDP should stay selected: ${JSON.stringify(remoteLegacy)}`);
  }
  const customLocal = planCdpEndpointRecovery(
    parseCdpEndpoint('http://127.0.0.1:9333'),
    { status: 'closed', reason: 'connection refused' },
  );
  if (customLocal.endpoint.origin !== 'http://127.0.0.1:9333' || customLocal.recovery) {
    throw new Error(`custom local CDP should keep existing behavior: ${JSON.stringify(customLocal)}`);
  }
  validateEnvelope({
    v: 1,
    type: 'hello',
    run_id: 'run-test',
    ts: timestamp(),
    payload: {},
  });
  assertEnvelopeRunIdValidation();
  assertErrorPayloadRedaction();
  assertBridgeLogProfileFieldRedaction();
  assertManagedChromeLaunchPlanning();
  assertArtifactStallDefaultDisabled();
  await assertProfileLockCleanup();
  await assertSessionExpiredFailFast();
  await assertEmailCodeSelectionAndManualAuthFallbacks();
  await assertComposerWaitAndAuthClassification();
  await assertKeepAliveCleanup();
  await assertLocalArchivePathSkipsGitArchive();
  await assertFreshSourceCloneArchivesLocalRepos();
  await assertDownloadCleanupSequencing();
  await assertNoTarCleanupSequencing('done-no-tar', 'assistant finished but no tar.gz download candidate was found');
  await assertNoTarCleanupSequencing('artifact-stall-no-tar', 'assistant stalled without a tar.gz download candidate');
  await assertNoTarCleanupSequencing('message-stream-no-tar', 'assistant hit message stream error without tar.gz after 0 retry attempts');
  await assertNoTarCleanupSequencing('timeout-no-tar', 'timed out after 30 minutes waiting for tar.gz download candidate');
  await assertNoLinkBundleCapture('done-no-tar', 'assistant finished but no tar.gz download candidate was found');
  await assertNoLinkBundleCapture('artifact-stall-no-tar', 'assistant stalled without a tar.gz download candidate');
  await assertNoLinkBundleCapture('message-stream-no-tar', 'assistant hit message stream error without tar.gz after 0 retry attempts');
  await assertNoLinkBundleCapture('timeout-no-tar', 'timed out after 30 minutes waiting for tar.gz download candidate');
  await assertDownloadSaveAsTempPathCopySucceeds();
  await assertDownloadSaveAsBrowserDownloadCopySucceeds();
  await assertDirectJsonDownloadPreservesTargetName();
  assertTextMaterializationValidation();
  await assertDownloadFailureDiagnosticsAndCleanup();
  await assertDirectTexDownloadFailureIsArtifactScoped();
  await assertMessageStreamRetryHardDisabled();
  await assertArtifactConversationLinkCollection();
  await assertArtifactConversationRecoveryDownloadsFromLinkedPage();
  await assertArtifactConversationRecoveryNoCandidateDiagnostics();
  await assertKnownRunUrlCollection();
  await assertBrowserProfilePoolPlanning();
  await assertManagedBrowserTerminationSequence();
  assertTransientNavigationErrorClassification();
  process.stdout.write('chrome-bridge self-test passed\n');
}

async function assertFreshSourceCloneArchivesLocalRepos() {
  const root = await mkdtemp(join(tmpdir(), 'jailgun-bridge-selftest-'));
  try {
    const repo = join(root, 'source');
    await mkdir(repo, { recursive: true });
    await runGit(['init'], repo);
    await runGit(['config', 'user.email', 'jailgun@example.test'], repo);
    await runGit(['config', 'user.name', 'Jailgun Self Test'], repo);
    await writeFile(join(repo, 'README.md'), '# source\n');
    await runGit(['add', 'README.md'], repo);
    await runGit(['commit', '-m', 'initial'], repo);

    const direct = await createSourceArchive({
      repoUrl: repo,
      refName: 'HEAD',
      prefix: 'source/',
      archiveFilename: 'source.tar.gz',
      tmpParent: root,
      mode: 'full',
      freshSourceClone: false,
    });
    if (direct.cloneDir !== '' || direct.freshSourceClone) {
      throw new Error(`local archive should use source checkout by default: ${JSON.stringify(direct)}`);
    }
    await rm(direct.tempRoot, { recursive: true, force: true });

    const fresh = await createSourceArchive({
      repoUrl: repo,
      refName: 'HEAD',
      prefix: 'source/',
      archiveFilename: 'source.tar.gz',
      tmpParent: root,
      mode: 'full',
      freshSourceClone: true,
    });
    if (!fresh.cloneDir || !fresh.freshSourceClone || !fresh.cloneDir.startsWith(fresh.tempRoot)) {
      throw new Error(`fresh local archive should clone into temp root: ${JSON.stringify(fresh)}`);
    }
    await rm(fresh.tempRoot, { recursive: true, force: true });
  } finally {
    await rm(root, { recursive: true, force: true }).catch(() => undefined);
  }
}

async function assertLocalArchivePathSkipsGitArchive() {
  const root = await mkdtemp(join(tmpdir(), 'jailgun-bridge-selftest-local-archive-'));
  try {
    const archivePath = await createSelfTestTarGz(root);
    const archive = await createSourceArchive({
      repoUrl: '',
      refName: 'HEAD',
      prefix: 'source/',
      archiveFilename: 'source.tar.gz',
      tmpParent: root,
      mode: 'full',
      freshSourceClone: false,
      localArchivePath: archivePath,
    });
    if (archive.archivePath !== archivePath || archive.tempRoot !== '' || archive.cloneDir !== '' || archive.commit !== 'local-archive') {
      throw new Error(`local_archive_path should use the supplied file directly: ${JSON.stringify(archive)}`);
    }
  } finally {
    await rm(root, { recursive: true, force: true }).catch(() => undefined);
  }
}

function assertArtifactStallDefaultDisabled() {
  if (DEFAULT_ARTIFACT_STALL_REPAIR_SECONDS !== 0) {
    throw new Error(`artifact stall cutoff should be opt-in, got ${DEFAULT_ARTIFACT_STALL_REPAIR_SECONDS}`);
  }
}

const SEND_BUTTON_SELECTORS = [
  'button[data-testid="send-button"]',
  'button[aria-label*="Send"]',
  '[data-testid*="send"]',
  'button:has-text("Send")',
];

const CHAT_COMPOSER_SELECTORS = [
  '#prompt-textarea',
  '[data-testid="composer-text-input"]',
  ['textarea[place', 'holder*="Message"]'].join(''),
  '[contenteditable="true"][role="textbox"]',
  'form [contenteditable="true"]',
];

const AUTH_CONTROL_SELECTOR = [
  'button',
  '[role="button"]',
  'a',
  'label',
  'input[type="button"]',
  'input[type="submit"]',
].join(', ');

const CODE_INPUT_SELECTOR = [
  'input[autocomplete="one-time-code"]',
  'input[name*="code" i]',
  'input[inputmode="numeric"]',
  'input[type="tel"]',
].join(', ');

const MARKDOWN_EXTENSIONS = new Set(['.md', '.mdx']);
const CODE_EXTENSIONS = new Set([
  '.bash',
  '.c',
  '.cc',
  '.cjs',
  '.cpp',
  '.cs',
  '.css',
  '.fish',
  '.go',
  '.graphql',
  '.h',
  '.hh',
  '.hpp',
  '.html',
  '.java',
  '.js',
  '.jsx',
  '.kt',
  '.kts',
  '.lua',
  '.mjs',
  '.nix',
  '.php',
  '.proto',
  '.py',
  '.rb',
  '.rs',
  '.scss',
  '.sh',
  '.sql',
  '.swift',
  '.tf',
  '.toml',
  '.ts',
  '.tsx',
  '.vim',
  '.yaml',
  '.yml',
]);
const CODE_FILENAMES = new Set([
  '.dockerignore',
  '.editorconfig',
  '.gitattributes',
  '.gitignore',
  'dockerfile',
  'justfile',
  'makefile',
  'package.json',
  'pyproject.toml',
  'requirements.in',
  'requirements.txt',
  'go.mod',
  'go.sum',
  'cargo.toml',
]);
const EXCLUDED_FILENAMES = new Set([
  'cargo.lock',
  'package-lock.json',
  'pnpm-lock.yaml',
  'poetry.lock',
  'yarn.lock',
]);
const EXCLUDED_DIRECTORIES = new Set([
  '.cache',
  '.git',
  '.next',
  '.nuxt',
  '.parcel-cache',
  '.svelte-kit',
  '.turbo',
  '.venv',
  'artifacts',
  'build',
  'coverage',
  'dist',
  'downloads',
  'logs',
  'node_modules',
  'out',
  'target',
  'tmp',
  'vendor',
]);

if (shouldSelfTest) {
  await runSelfTest();
  process.exit(0);
}

const bridge = new ChromeBridge(settings);
installSignalHandlers(bridge);
await bridge.run();

function installSignalHandlers(bridgeInstance) {
  const exits = new Map([
    ['SIGHUP', 129],
    ['SIGINT', 130],
    ['SIGTERM', 143],
  ]);
  for (const [signal, code] of exits.entries()) {
    process.once(signal, () => {
      void bridgeInstance
        .shutdown(`signal-${signal}`, 0, bridgeInstance.lastEnvelope ?? systemEnvelope(`signal-${signal}`))
        .finally(() => {
          process.exit(code);
        });
    });
  }
}

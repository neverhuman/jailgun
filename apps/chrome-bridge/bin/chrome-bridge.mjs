#!/usr/bin/env node
import { createHash } from 'node:crypto';
import http from 'node:http';
import net from 'node:net';
import { createWriteStream, existsSync } from 'node:fs';
import { mkdir, mkdtemp, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { homedir, tmpdir } from 'node:os';
import { basename, extname, isAbsolute, join, resolve } from 'node:path';
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
const DEFAULT_GLOBAL_MODAL_SWEEP_MS = 2500;
const DEFAULT_MESSAGE_STREAM_RETRY_LIMIT = 6;
const DEFAULT_MESSAGE_STREAM_RETRY_DELAY_MS = 10000;

const args = parseArgs(process.argv.slice(2));
if (args.selfTest === 'true') {
  await runSelfTest();
  process.exit(0);
}

const cdpUrlSetting = firstSetting([
  ['--cdp-url', args.cdpUrl],
  ['JAILGUN_CDP_URL', process.env.JAILGUN_CDP_URL],
]);
const cdpHostSetting = firstSetting([
  ['--cdp-host', args.cdpHost],
  ['--host', args.host],
  ['JAILGUN_CDP_HOST', process.env.JAILGUN_CDP_HOST],
  ['GOOGLE_AUTOMATION_REMOTE_DEBUG_HOST', process.env.GOOGLE_AUTOMATION_REMOTE_DEBUG_HOST],
]);
const cdpPortSetting = firstSetting([
  ['--cdp-port', args.cdpPort],
  ['--port', args.port],
  ['JAILGUN_CDP_PORT', process.env.JAILGUN_CDP_PORT],
  ['GOOGLE_AUTOMATION_REMOTE_DEBUG_PORT', process.env.GOOGLE_AUTOMATION_REMOTE_DEBUG_PORT],
]);
const cdpUrlOverride = cdpUrlSetting?.value ?? null;
const cdpHost = cdpHostSetting?.value ?? DEFAULT_CDP_HOST;
const cdpPort = numberFrom(cdpPortSetting?.value, DEFAULT_CDP_PORT);

const settings = {
  cdpUrl: cdpUrlOverride ?? `http://${cdpHost}:${cdpPort}`,
  cdpEndpointSource: cdpUrlSetting?.source ?? cdpPortSetting?.source ?? cdpHostSetting?.source ?? 'default',
  cdpEndpointConfigured: Boolean(cdpUrlSetting || cdpPortSetting || cdpHostSetting),
  profileDir: resolvePath(args.profileDir ?? process.env.JAILGUN_CHROME_PROFILE_DIR ?? process.env.GOOGLE_AUTOMATION_PROFILE_DIR ?? DEFAULT_PROFILE_DIR),
  stateDir: resolvePath(args.stateDir ?? process.env.JAILGUN_CHROME_STATE_DIR ?? process.env.GOOGLE_AUTOMATION_STATE_DIR ?? DEFAULT_STATE_DIR),
  chromeExecutable: args.chromeExecutable ?? args.browserExecutable ?? process.env.JAILGUN_CHROME_EXECUTABLE ?? process.env.GOOGLE_CHROME_EXECUTABLE ?? '',
  browserTimeoutMs: numberFrom(args.browserTimeoutMs ?? args.timeoutMs ?? process.env.JAILGUN_CHROME_TIMEOUT_MS ?? process.env.GOOGLE_AUTOMATION_TIMEOUT_MS, DEFAULT_BROWSER_TIMEOUT_MS),
  downloadsDir: resolvePath(args.downloadsDir ?? process.env.JAILGUN_DOWNLOADS_DIR ?? join(homedir(), 'Downloads')),
  artifactsDir: resolvePath(args.artifactsDir ?? process.env.JAILGUN_ARTIFACTS_DIR ?? 'artifacts'),
  sourceMode: args.sourceMode ?? process.env.JAILGUN_SOURCE_ARCHIVE_MODE ?? DEFAULT_SOURCE_ARCHIVE_MODE,
  tarTargetName: args.tarTargetName ?? process.env.JAILGUN_TAR_TARGET_NAME ?? '',
  submitDelaySeconds: numberFrom(args.submitDelaySeconds ?? process.env.JAILGUN_SUBMIT_DELAY_SECONDS, 0),
  submitJitterSeconds: numberFrom(args.submitJitterSeconds ?? process.env.JAILGUN_SUBMIT_JITTER_SECONDS, 0),
  tarWaitMinutes: numberFrom(args.tarWaitMinutes ?? process.env.JAILGUN_TAR_WAIT_MINUTES, DEFAULT_MAX_MINUTES),
  globalModalSweepMs: numberFrom(args.globalModalSweepMs ?? process.env.JAILGUN_GLOBAL_MODAL_SWEEP_MS, DEFAULT_GLOBAL_MODAL_SWEEP_MS),
  messageStreamRetryLimit: numberFrom(args.messageStreamRetryLimit ?? process.env.JAILGUN_MESSAGE_STREAM_RETRY_LIMIT, DEFAULT_MESSAGE_STREAM_RETRY_LIMIT),
  messageStreamRetryDelayMs: numberFrom(
    args.messageStreamRetryDelayMs ?? process.env.JAILGUN_MESSAGE_STREAM_RETRY_DELAY_MS,
    DEFAULT_MESSAGE_STREAM_RETRY_DELAY_MS,
  ),
  recoverKnownRunTabs: booleanFrom(args.recoverKnownRunTabs ?? process.env.JAILGUN_RECOVER_KNOWN_RUN_TABS, true),
  knownRunArtifactsDir: resolvePath(args.knownRunArtifactsDir ?? process.env.JAILGUN_KNOWN_RUN_ARTIFACTS_DIR ?? join('artifacts', 'live-runs')),
};

class ChromeBridge {
  constructor(options) {
    this.options = options;
    this.browser = null;
    this.browserEndpoint = null;
    this.context = null;
    this.tabs = new Map();
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
      const chrome = await this.ensureBrowser(envelope);
      this.emit(envelope, 'bridge-ready', {
        node_version: process.version,
        playwright_version: PLAYWRIGHT_VERSION,
        browser: 'chromium-cdp',
        browser_version: await this.browser.version(),
        cdp_url: chrome.cdpUrl,
        managed_chrome_started: chrome.started,
        capabilities: [
          'managed-chrome',
          'source-upload',
          'prompt-submit-readiness',
          'tar-capture',
          'rate-limit-detection',
          'global-modal-sweeper',
          'known-run-tab-recovery',
          'message-stream-retry',
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

  async ensureBrowser(envelope = null) {
    if (this.browser && this.context) {
      return this.browserEndpoint;
    }
    const logStartup = envelope
      ? (phase, status, message, fields, level) => this.bridgeLog(envelope, phase, status, message, fields, level)
      : null;
    const chrome = await ensureManagedChromeRunning(this.options, logStartup);
    this.browser = await chromium.connectOverCDP(chrome.cdpUrl, { timeout: this.options.browserTimeoutMs });
    this.context = this.browser.contexts()[0];
    if (!this.context) {
      throw new Error(`no browser context found at ${chrome.cdpUrl}`);
    }
    this.browserEndpoint = chrome;
    return chrome;
  }

  async openTab(envelope) {
    await this.ensureBrowser();
    const tabId = requiredTabId(envelope);
    const payload = envelope.payload ?? {};
    const page = await this.context.newPage();
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
    });
    this.bridgeLog(envelope, 'open-tab', 'ok', 'tab opened', {
      page_url: page.url(),
      model: payload.model || '',
    });
    this.emit(envelope, 'tab-opened', {
      page_url: page.url(),
      page_id: `tab-${String(tabId).padStart(2, '0')}`,
    }, tabId);
  }

  async uploadArchive(envelope) {
    const tab = this.requireTab(envelope);
    const payload = envelope.payload ?? {};
    this.bridgeLog(envelope, 'source-upload', 'started', 'creating source archive', {
      repo_url: payload.repo_url || '',
      ref_name: payload.ref_name || 'HEAD',
    });
    const archive = await createSourceArchive({
      repoUrl: requiredString(payload.repo_url, 'repo_url'),
      refName: payload.ref_name || 'HEAD',
      prefix: payload.prefix || 'source/',
      archiveFilename: payload.archive_filename || 'source.tar.gz',
      tmpParent: payload.tmp_parent || undefined,
      mode: this.options.sourceMode,
    });

    let deletedTemp = false;
    try {
      await uploadFileToChat(tab.page, archive.archivePath, payload.timeout_ms ?? 45000);
      await confirmUpload(tab.page, archive.archiveFilename, payload.confirm_selectors ?? [], payload.timeout_ms ?? 45000);
      const fileStat = await stat(archive.archivePath);
      const sha256 = await sha256File(archive.archivePath);
      if (payload.delete_after_upload !== false) {
        await rm(archive.tempRoot, { recursive: true, force: true });
        deletedTemp = true;
      }
      this.emit(envelope, 'archive-uploaded', {
        sha256,
        size_bytes: fileStat.size,
        commit: archive.commit,
        archive_filename: archive.archiveFilename,
        deleted_temp: deletedTemp,
      });
      this.bridgeLog(envelope, 'source-upload', 'ok', 'source archive uploaded', {
        sha256,
        size_bytes: String(fileStat.size),
        archive_filename: archive.archiveFilename,
      });
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
    await mkdir(outputDir, { recursive: true });
    let lastTelemetry = 0;
    let tick = 0;
    let messageStreamRetries = 0;
    this.bridgeLog(envelope, 'monitor-started', 'ok', 'tab monitor loop started', {
      completion_check_ms: String(completionMs),
      telemetry_tick_ms: String(pollMs),
      deadline: new Date(deadline).toISOString(),
      page_url: tab.page.url(),
    });

    while (!this.shutdownRequested && Date.now() <= deadline) {
      tick += 1;
      await this.runDismissals(tab.page, envelope, 'monitor-dismissals');
      await this.handleGitHubToolPrompts(tab.page, envelope);

      const discovery = await discoverTarCandidates(tab.page);
      const status = await readGenerationStatus(tab.page);
      const ranked = rankCandidates(discovery.candidates, this.options.tarTargetName);
      const now = Date.now();
      const progressKind = now - lastTelemetry >= pollMs ? 'telemetry' : 'completion-check';
      if (progressKind === 'telemetry') {
        lastTelemetry = now;
      }
      this.emit(envelope, 'tab-progress', {
        kind: progressKind,
        phase: ranked.length > 0 ? 'tar-candidate-found' : status.messageStreamError ? 'message-stream-error' : status.activeStop ? 'generating' : 'checking',
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
          preview: compact(discovery.lastTextPreview || '', 160),
          page_url: tab.page.url(),
        });
      }

      if (ranked.length > 0) {
        const candidate = ranked[0];
        this.emit(envelope, 'tar-discovered', {
          candidates: ranked.slice(0, 5),
          selected_index: candidate.index,
        });
        const startedDownloadAt = timestamp();
        const targetPath = join(outputDir, normalizeTarName(candidate.label || candidate.download || candidate.href || 'chatgpt-output.tar.gz'));
        this.emit(envelope, 'download-started', {
          candidate_index: candidate.index,
          remote_url: candidate.href || '',
          target_path: targetPath,
          started_at: startedDownloadAt,
        });
        this.bridgeLog(envelope, 'download-started', 'started', 'clicking selected tar download candidate', {
          candidate_index: String(candidate.index),
          candidate_count: String(ranked.length),
          candidate_score: String(candidate.score ?? ''),
          target_path: targetPath,
          label: compact(candidate.label || candidate.download || candidate.href || '', 160),
        });
        let completePayload = null;
        let cleanup = null;
        let cleanupReason = 'download-failed';
        try {
          await this.runDismissals(tab.page, envelope, 'download-preflight');
          const file = await downloadCandidate(tab.page, candidate, outputDir);
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
            local_name: file.suggested,
            download_url: candidate.href || null,
            entry_count: file.entryCount,
            started_at: startedDownloadAt,
            finished_at: finishedDownloadAt,
            download_latency_ms: downloadLatencyMs,
          };
          await writeFile(receiptPath, JSON.stringify(completePayload, null, 2));
          cleanupReason = 'download-complete';
        } finally {
          cleanup = await finalizeTabAfterDownload(this, tab, envelope, cleanupReason);
        }
        this.emit(envelope, 'download-complete', completePayload);
        this.bridgeLog(envelope, 'download-complete', 'ok', 'download receipt written and tab closed', {
          sha256: completePayload.sha256,
          size_bytes: String(completePayload.size_bytes),
          entry_count: String(completePayload.entry_count),
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

      if (status.messageStreamError && messageStreamRetries < this.options.messageStreamRetryLimit) {
        messageStreamRetries += 1;
        const retry = await retryMessageStreamError(tab.page);
        this.bridgeLog(
          envelope,
          'message-stream-retry',
          retry.clicked ? 'clicked' : 'not-clicked',
          retry.clicked ? 'clicked ChatGPT message stream Retry' : 'message stream error detected but Retry was not clicked',
          {
            attempt: String(messageStreamRetries),
            max_attempts: String(this.options.messageStreamRetryLimit),
            detected: String(Boolean(retry.detected)),
            button_label: retry.buttonLabel || '',
            reason: retry.reason || '',
            excerpt: compact(retry.excerpt || '', 200),
          },
          retry.clicked ? 'warn' : 'error',
        );
        if (retry.clicked) {
          await sleep(this.options.messageStreamRetryDelayMs);
          continue;
        }
      }

      if (status.messageStreamError) {
        await emitNoTarErrorAndCleanup(
          this,
          tab,
          envelope,
          'message-stream-no-tar',
          `assistant hit message stream error without tar.gz after ${messageStreamRetries} retry attempts`,
        );
        return;
      }

      if (!status.activeStop && status.finalActions > 0) {
        await emitNoTarErrorAndCleanup(
          this,
          tab,
          envelope,
          'done-no-tar',
          'assistant finished but no tar.gz download candidate was found',
        );
        return;
      }

      await sleep(Math.min(completionMs, pollMs));
    }

    await emitNoTarErrorAndCleanup(
      this,
      tab,
      envelope,
      'timeout-no-tar',
      `timed out after ${this.options.tarWaitMinutes} minutes waiting for tar.gz download candidate`,
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
    if (!this.context || this.globalDismissalRunning) {
      return { pages: 0, dismissed: 0 };
    }
    this.globalDismissalRunning = true;
    let pages = 0;
    let dismissed = 0;
    try {
      for (const page of this.context.pages()) {
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
          },
          'warn',
        );
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
    if (!this.context || !this.options.recoverKnownRunTabs) {
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
    for (const page of this.context.pages()) {
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

  async stopGeneration(envelope) {
    const tab = this.requireTab(envelope);
    const result = await stopIfGenerating(tab.page);
    if (result.clicked) {
      this.emit(envelope, 'generation-stopped', { method: result.label || 'button' });
    }
  }

  async closeTab(envelope, reason) {
    const tab = this.requireTab(envelope);
    const pageUrl = tab.page.url();
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
    await tab.page.close({ runBeforeUnload: false }).catch(() => undefined);
    this.emit(envelope, 'tab-closed', {
      page_url: pageUrl,
      reason,
    });
    tab.page = null;
    return true;
  }

  requireTab(envelope) {
    const tabId = requiredTabId(envelope);
    const tab = this.tabs.get(tabId);
    if (!tab?.page) {
      throw new Error(`tab ${tabId} is not open`);
    }
    return tab;
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
    await this.browser?.close().catch(() => undefined);
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
    const normalizedFields = {};
    for (const [key, value] of Object.entries(fields || {})) {
      if (value !== undefined && value !== null) {
        normalizedFields[key] = String(value);
      }
    }
    normalizedFields.status = status;
    this.emit(envelope, 'bridge-log', {
      level,
      phase,
      message,
      fields: normalizedFields,
    });
    process.stderr.write(formatBridgeStderr(envelope, phase, status, message, normalizedFields, level));
  }

  logError(phase, error) {
    process.stderr.write(`[chrome-bridge] ${phase}: ${error?.stack || error?.message || String(error)}\n`);
  }
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
    return { cdpUrl: endpoint.origin, started: false };
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

  const child = spawn(executable, [
    `--user-data-dir=${options.profileDir}`,
    '--profile-directory=Default',
    '--no-first-run',
    '--no-default-browser-check',
    '--new-window',
    `--remote-debugging-port=${endpoint.port}`,
    `--remote-debugging-address=${endpoint.hostname}`,
    'about:blank',
  ], {
    detached: true,
    stdio: 'ignore',
    env: { ...process.env },
  });
  child.unref();

  await writeManagedBrowserState(options.stateDir, {
    pid: child.pid,
    host: endpoint.hostname,
    port: endpoint.port,
    profileDir: options.profileDir,
    executable,
    startedAt: timestamp(),
  });

  try {
    await waitForCdpVersion(endpoint, options.browserTimeoutMs);
  } catch (error) {
    const lockArtifacts = detectProfileLockArtifacts(options.profileDir);
    if (lockArtifacts.length > 0) {
      error.message += `\nThe managed Chrome profile appears to be locked. Close any regular Chrome window using this profile and retry.\nProfile lock hints:\n- ${lockArtifacts.join('\n- ')}`;
    }
    throw error;
  }

  return { cdpUrl: endpoint.origin, started: true };
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

  throw new Error('Could not find Google Chrome on this Mac. Install Google Chrome or set JAILGUN_CHROME_EXECUTABLE to the full executable path.');
}

function chromeExecutableCandidates() {
  const home = homedir();
  const candidates = [
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

async function writeManagedBrowserState(stateDir, state) {
  await mkdir(stateDir, { recursive: true });
  await writeFile(join(stateDir, 'managed-browser.pid'), `${state.pid ?? ''}\n`);
  await writeFile(join(stateDir, 'managed-browser.json'), JSON.stringify(state, null, 2));
}

async function createSourceArchive(options) {
  validateArchiveOptions(options);
  const tmpParent = options.tmpParent ?? tmpdir();
  await mkdir(tmpParent, { recursive: true });
  const tempRoot = await mkdtemp(join(tmpParent, 'jailgun-source-'));
  const archivePath = join(tempRoot, basename(options.archiveFilename));
  let repoDir = null;
  let cleanupRepo = false;
  try {
    const local = await localRepoPath(options.repoUrl);
    if (local) {
      repoDir = local;
    } else {
      repoDir = join(tempRoot, 'repo');
      cleanupRepo = true;
      await runGit(['clone', '--depth=1', options.repoUrl, repoDir]);
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
      archivePath,
      archiveFilename: basename(archivePath),
      commit,
    };
  } catch (error) {
    await rm(tempRoot, { recursive: true, force: true }).catch(() => undefined);
    throw error;
  }
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
  if (!options.repoUrl?.trim()) {
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
    `text=${filename}`,
    `[aria-label*="${cssAttr(filename)}"]`,
    `[title*="${cssAttr(filename)}"]`,
    '[data-testid*="attachment"]',
  ];
  let lastError = null;
  for (const selector of selectors) {
    try {
      await page.waitForSelector(selector, { timeout: Math.min(timeoutMs, 10000) });
      return;
    } catch (error) {
      lastError = error;
    }
  }
  throw new Error(`uploaded archive was not confirmed in chat UI: ${lastError?.message || lastError}`);
}

async function submitPromptToChat(page, prompt, timeoutMs, hooks = {}) {
  const startedAt = Date.now();
  hooks.log?.('prompt-submit-wait', 'started', 'locating composer for prompt submission', {
    prompt_bytes: String(Buffer.byteLength(prompt, 'utf8')),
  });
  const composer = await firstAvailableLocator(page, [
    '#prompt-textarea',
    '[data-testid="composer-text-input"]',
    ['textarea[place', 'holder*="Message"]'].join(''),
    '[contenteditable="true"][role="textbox"]',
    'form [contenteditable="true"]',
  ]);

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

async function discoverTarCandidates(page) {
  return page.evaluate(() => {
    const controls = Array.from(document.querySelectorAll('a,button,[role="button"],[download],[href]'));
    const assistantRoots = Array.from(document.querySelectorAll('[data-message-author-role="assistant"]'));
    const textOf = (el) => String(el?.innerText || el?.textContent || '').replace(/\s+/g, ' ').trim();
    const attr = (el, name) => el?.getAttribute?.(name) || '';
    const href = (el) => el?.href || attr(el, 'href');
    const closestAssistant = (el) => el?.closest?.('[data-message-author-role="assistant"]') || null;
    const visible = (el) => {
      const style = window.getComputedStyle(el);
      const rect = el.getBoundingClientRect();
      return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
    };
    const disabled = (el) => el.hasAttribute?.('disabled') || /^true$/i.test(attr(el, 'aria-disabled'));
    const tar = (value) => /\.tar\.gz(?:$|[?#\s)]|\.tar\(\d+\)\.gz)/i.test(String(value || ''));
    const candidates = [];
    for (let index = 0; index < controls.length; index += 1) {
      const el = controls[index];
      const assistant = closestAssistant(el);
      if (assistantRoots.length > 0 && !assistant) continue;
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
      if (!tar(haystack)) continue;
      entry.label = entry.text || entry.download || entry.href || entry.aria || entry.title;
      entry.score += 200;
      if (/download/i.test(haystack)) entry.score += 100;
      if (tar(entry.download)) entry.score += 90;
      if (tar(entry.href)) entry.score += 80;
      if (tar(entry.text)) entry.score += 60;
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
    };
  });
}

function rankCandidates(candidates, targetName) {
  const target = String(targetName || '').toLowerCase();
  return [...candidates]
    .map((candidate) => {
      if (target) {
        const haystack = `${candidate.text} ${candidate.href} ${candidate.download} ${candidate.aria} ${candidate.title}`.toLowerCase();
        if (haystack.includes(target)) {
          return { ...candidate, score: candidate.score + 500 };
        }
      }
      return candidate;
    })
    .sort((a, b) => b.score - a.score);
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

async function retryMessageStreamError(page) {
  try {
    return await page.evaluate(() => {
      const controls = Array.from(document.querySelectorAll('button,[role="button"],a,[aria-label],[title]'));
      const textOf = (el) => String(el?.innerText || el?.textContent || '').replace(/\s+/g, ' ').trim();
      const pageText = textOf(document.body);
      const detected = /error in message stream/i.test(pageText);
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
      if (!detected) {
        return { detected: false, clicked: false, buttonLabel: '', excerpt: '', reason: 'message-stream-error-not-detected' };
      }
      for (const el of controls) {
        if (!visible(el) || disabled(el)) continue;
        const text = label(el);
        if (!/^\s*retry\s*$/i.test(text)) continue;
        el.click();
        return { detected: true, clicked: true, buttonLabel: text, excerpt: pageText.slice(0, 240), reason: '' };
      }
      return { detected: true, clicked: false, buttonLabel: '', excerpt: pageText.slice(0, 240), reason: 'retry-control-not-found' };
    });
  } catch (error) {
    return {
      detected: false,
      clicked: false,
      buttonLabel: '',
      excerpt: '',
      reason: `evaluate-failed: ${error.message}`,
    };
  }
}

async function downloadCandidate(page, candidate, outputDir, timeoutMs = 120000) {
  const downloadPromise = page.waitForEvent('download', { timeout: timeoutMs });
  const locator = page.locator('a,button,[role="button"],[download],[href]').nth(candidate.index);
  await locator.scrollIntoViewIfNeeded({ timeout: 5000 }).catch(() => undefined);
  await locator.click({ timeout: timeoutMs });
  const download = await downloadPromise;
  const suggested = normalizeTarName(download.suggestedFilename() || basename(candidate.href || '') || 'chatgpt-output.tar.gz');
  const path = join(outputDir, suggested);
  await mkdir(outputDir, { recursive: true });
  await download.saveAs(path);
  const failure = await download.failure();
  if (failure) {
    throw new Error(`download failed: ${failure}`);
  }
  const fileStat = await stat(path);
  if (!fileStat.isFile() || fileStat.size === 0) {
    throw new Error(`downloaded file was empty or not a file: ${path}`);
  }
  const sha256 = await sha256File(path);
  const tarList = spawnSync('tar', ['-tzf', path], { encoding: 'utf8' });
  if (tarList.status !== 0) {
    throw new Error(`downloaded file is not a valid tar.gz: ${tarList.stderr.trim()}`);
  }
  const entryCount = tarList.stdout.split(/\r?\n/).map((line) => line.trim()).filter(Boolean).length;
  if (entryCount === 0) {
    throw new Error(`downloaded file has zero tar entries: ${path}`);
  }
  return {
    path,
    suggested,
    sizeBytes: fileStat.size,
    sha256,
    entryCount,
  };
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
            reason: 'detect-only',
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
      bridge.emit(envelope, 'generation-stopped', { method: stopMethod });
      bridge.bridgeLog(
        envelope,
        'generation-stopped',
        stop.clicked ? 'ok' : 'not-active',
        stop.clicked ? `stopped generation ${context}` : `generation was not active ${context}`,
        { method: stopMethod },
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

async function emitNoTarErrorAndCleanup(bridge, tab, envelope, kind, message) {
  const cleanup = await finalizeTabAfterDownload(bridge, tab, envelope, kind);
  bridge.emit(envelope, 'error', {
    kind,
    message,
    recoverable: false,
    stack: null,
    cleanup_stop_method: cleanup.stopMethod,
    tab_closed: cleanup.closed,
    cleanup_errors: cleanup.errors.join(';'),
  });
  bridge.bridgeLog(envelope, kind, 'failed', message, {
    cleanup_stop_method: cleanup.stopMethod,
    tab_closed: String(Boolean(cleanup.closed)),
    cleanup_errors: cleanup.errors.join(';'),
  }, 'error');
  return cleanup;
}

function terminalCleanupContext(reason) {
  if (reason === 'download-complete') {
    return 'after tar receipt';
  }
  if (reason === 'download-failed') {
    return 'after failed tar download';
  }
  if (reason === 'done-no-tar') {
    return 'after assistant finished without a tar';
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
    const discovery = await discoverTarCandidates(page);
    const ranked = rankCandidates(discovery.candidates, bridge.options.tarTargetName);
    if (ranked.length > 0) {
      const candidate = ranked[0];
      bridge.bridgeLog(envelope, phase, 'download-started', 'recovering download from known abandoned run tab', {
        source_run_id: source.runId || '',
        source_tab_id: source.tabId == null ? '' : String(source.tabId),
        page_url: pageUrl,
        candidate_index: String(candidate.index),
        candidate_count: String(ranked.length),
        output_dir: outputDir,
      }, 'warn');
      const file = await downloadCandidate(page, candidate, outputDir, 30000);
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
        local_name: file.suggested,
        sha256: file.sha256,
        size_bytes: file.sizeBytes,
        entry_count: file.entryCount,
        recovered_at: timestamp(),
      }, null, 2));
      bridge.bridgeLog(envelope, phase, 'downloaded', 'recovered tar download from known abandoned run tab', {
        source_run_id: source.runId || '',
        source_tab_id: source.tabId == null ? '' : String(source.tabId),
        page_url: pageUrl,
        local_path: file.path,
        receipt_path: receiptPath,
        sha256: file.sha256,
        size_bytes: String(file.sizeBytes),
        entry_count: String(file.entryCount),
      }, 'warn');
    } else {
      bridge.bridgeLog(envelope, phase, 'no-candidate', 'known abandoned run tab had no tar candidate during recovery', {
        source_run_id: source.runId || '',
        source_tab_id: source.tabId == null ? '' : String(source.tabId),
        page_url: pageUrl,
        scanned_control_count: String(discovery.scannedControlCount ?? 0),
      }, 'warn');
    }
  } catch (error) {
    bridge.bridgeLog(envelope, phase, 'download-failed', 'failed to recover tar from known abandoned run tab', {
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
    message: error?.message || String(error),
    recoverable: false,
    stack: error?.stack || null,
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

function compact(value, max = 240) {
  const text = String(value || '').replace(/\s+/g, ' ').trim();
  return text.length > max ? `${text.slice(0, Math.max(0, max - 3))}...` : text;
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
  const text = String(value);
  if (text === '') {
    return '""';
  }
  if (/^[A-Za-z0-9._~:/@%+=,-]+$/.test(text)) {
    return text;
  }
  return JSON.stringify(text);
}

function cssAttr(value) {
  return value.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
}

function normalizeTarName(value) {
  const safe = String(value || 'chatgpt-output.tar.gz').replace(/[/\\]/g, '-');
  const normalized = safe
    .replace(/\.tar\(\d+\)\.gz$/i, '.tar.gz')
    .replace(/\.tgz$/i, '.tar.gz')
    .replace(/\.gz\.tar\.gz$/i, '.gz');
  return /\.tar\.gz$/i.test(normalized) ? normalized : `${normalized}.tar.gz`;
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

async function assertNoTarCleanupSequencing() {
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
      bridge.emit(envelope, 'tab-closed', { page_url: 'https://chatgpt.com/c/test', reason: 'done-no-tar' });
      tab.page = null;
      return true;
    },
  };

  const cleanup = await emitNoTarErrorAndCleanup(
    bridge,
    tab,
    envelope,
    'done-no-tar',
    'assistant finished but no tar.gz download candidate was found',
  );
  const expected = [
    'stopIfGenerating',
    'emit:generation-stopped',
    'closeTabAfterReceipt',
    'emit:tab-closed',
    'emit:error',
  ];
  if (JSON.stringify(calls) !== JSON.stringify(expected)) {
    throw new Error(`no-tar cleanup sequence failed: ${JSON.stringify(calls)}`);
  }
  if (!cleanup.closed || cleanup.stopMethod !== 'not-active:not-found' || cleanup.errors.length > 0) {
    throw new Error(`no-tar cleanup result failed: ${JSON.stringify(cleanup)}`);
  }
}

async function assertMessageStreamRetryClicksRetry() {
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
  const retry = await retryMessageStreamError(page);
  if (!retry.clicked || !clicked || retry.buttonLabel !== 'Retry') {
    throw new Error(`message stream retry click failed: ${JSON.stringify({ retry, clicked })}`);
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

async function runSelfTest() {
  const name = normalizeTarName('jekko-fixes.tgz');
  if (name !== 'jekko-fixes.tar.gz') {
    throw new Error(`normalizeTarName failed: ${name}`);
  }
  const ranked = rankCandidates([
    { score: 1, text: 'other.tar.gz', href: '', download: '', aria: '', title: '' },
    { score: 1, text: 'Download jekko-fixes.tar.gz', href: '', download: '', aria: '', title: '' },
  ], 'jekko-fixes.tar.gz');
  if (!ranked[0].text.includes('jekko-fixes')) {
    throw new Error('target tar ranking failed');
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
  await assertDownloadCleanupSequencing();
  await assertNoTarCleanupSequencing();
  await assertMessageStreamRetryClicksRetry();
  await assertKnownRunUrlCollection();
  process.stdout.write('chrome-bridge self-test passed\n');
}

const SEND_BUTTON_SELECTORS = [
  'button[data-testid="send-button"]',
  'button[aria-label*="Send"]',
  '[data-testid*="send"]',
  'button:has-text("Send")',
];

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

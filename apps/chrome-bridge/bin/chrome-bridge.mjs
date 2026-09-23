#!/usr/bin/env node
import { createHash } from 'node:crypto';
import http from 'node:http';
import net from 'node:net';
import { createWriteStream, existsSync } from 'node:fs';
import { mkdir, mkdtemp, readFile, rm, stat, writeFile } from 'node:fs/promises';
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
const DEFAULT_PROFILE_DIR = join(homedir(), '.google-profile-automation-profile');
const DEFAULT_STATE_DIR = join(homedir(), '.google-profile-automation-state');
const DEFAULT_SOURCE_ARCHIVE_MODE = 'ai-source';
const DEFAULT_MAX_MINUTES = 30;
const DEFAULT_BROWSER_TIMEOUT_MS = 45000;

const args = parseArgs(process.argv.slice(2));
if (args.selfTest === 'true') {
  await runSelfTest();
  process.exit(0);
}

const cdpUrlOverride = args.cdpUrl ?? process.env.JAILGUN_CDP_URL ?? null;
const cdpHost = args.cdpHost ?? args.host ?? process.env.JAILGUN_CDP_HOST ?? process.env.GOOGLE_AUTOMATION_REMOTE_DEBUG_HOST ?? DEFAULT_CDP_HOST;
const cdpPort = numberFrom(args.cdpPort ?? args.port ?? process.env.JAILGUN_CDP_PORT ?? process.env.GOOGLE_AUTOMATION_REMOTE_DEBUG_PORT, DEFAULT_CDP_PORT);

const settings = {
  cdpUrl: cdpUrlOverride ?? `http://${cdpHost}:${cdpPort}`,
  cdpUrlExplicit: Boolean(cdpUrlOverride),
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
};

class ChromeBridge {
  constructor(options) {
    this.options = options;
    this.browser = null;
    this.context = null;
    this.tabs = new Map();
    this.shutdownRequested = false;
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

    await this.shutdown('stdin-closed', 0);
  }

  async handleLine(line) {
    if (!line.trim()) {
      return;
    }
    let envelope;
    try {
      envelope = JSON.parse(line);
      validateEnvelope(envelope);
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
      await this.ensureBrowser();
      this.emit(envelope, 'bridge-ready', {
        node_version: process.version,
        playwright_version: PLAYWRIGHT_VERSION,
        browser: 'chromium-cdp',
        browser_version: await this.browser.version(),
        capabilities: [
          'managed-chrome',
          'source-upload',
          'prompt-submit-readiness',
          'tar-capture',
          'rate-limit-detection',
        ],
      });
    } catch (error) {
      this.logError('startup-failed', error);
      this.emit(envelope, 'error', errorPayload('bridge-startup-failed', error));
      await this.shutdown('bridge-startup-failed', 0);
      process.exitCode = 1;
      setImmediate(() => process.exit(1));
    }
  }

  async ensureBrowser() {
    if (this.browser && this.context) {
      return;
    }
    const chrome = await ensureManagedChromeRunning(this.options);
    this.browser = await chromium.connectOverCDP(chrome.cdpUrl, { timeout: this.options.browserTimeoutMs });
    this.context = this.browser.contexts()[0];
    if (!this.context) {
      throw new Error(`no browser context found at ${chrome.cdpUrl}`);
    }
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
        phase: ranked.length > 0 ? 'tar-candidate-found' : status.activeStop ? 'generating' : 'checking',
        busy_reason: status.activeStop ? 'active-stop-button' : null,
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
        await this.runDismissals(tab.page, envelope, 'download-preflight');
        const file = await downloadCandidate(tab.page, candidate, outputDir);
        const receiptPath = join(this.options.artifactsDir, 'receipts', envelope.run_id, `tab-${String(tabId).padStart(2, '0')}-download.json`);
        await mkdir(resolve(receiptPath, '..'), { recursive: true });
        const completePayload = {
          sha256: file.sha256,
          size_bytes: file.sizeBytes,
          local_path: file.path,
          receipt_path: receiptPath,
          original_name: file.suggested,
          local_name: file.suggested,
          download_url: candidate.href || null,
          entry_count: file.entryCount,
          started_at: startedDownloadAt,
          finished_at: timestamp(),
        };
        await writeFile(receiptPath, JSON.stringify(completePayload, null, 2));
        const stop = await stopIfGenerating(tab.page);
        if (stop.clicked) {
          this.emit(envelope, 'generation-stopped', { method: stop.label || 'button' });
          this.bridgeLog(envelope, 'generation-stopped', 'ok', 'stopped generation after tar receipt', {
            method: stop.label || 'button',
          });
        }
        const closed = await this.closeTabAfterReceipt(tab, envelope, 'download-complete');
        this.emit(envelope, 'download-complete', completePayload);
        this.bridgeLog(envelope, 'download-complete', 'ok', 'download receipt written and tab closed', {
          sha256: file.sha256,
          size_bytes: String(file.sizeBytes),
          entry_count: String(file.entryCount),
          receipt_path: receiptPath,
          local_path: file.path,
          tab_closed: String(closed),
        });
        return;
      }

      if (!status.activeStop && status.finalActions > 0) {
        this.emit(envelope, 'error', {
          kind: 'done-no-tar',
          message: 'assistant finished but no tar.gz download candidate was found',
          recoverable: false,
          stack: null,
        });
        return;
      }

      await sleep(Math.min(completionMs, pollMs));
    }

    this.emit(envelope, 'error', {
      kind: 'timeout-no-tar',
      message: `timed out after ${this.options.tarWaitMinutes} minutes waiting for tar.gz download candidate`,
      recoverable: false,
      stack: null,
    });
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
    for (const tab of this.tabs.values()) {
      if (tab.page && !tab.page.isClosed()) {
        await tab.page.close().catch(() => undefined);
      }
    }
    if (envelope) {
      this.emit(envelope, 'bridge-shutting-down', { reason });
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

async function ensureManagedChromeRunning(options) {
  const endpoint = parseCdpEndpoint(options.cdpUrl);
  rejectKnownBadCdpEndpoint(endpoint, options.cdpUrlExplicit);

  const portOpen = await isPortOpen(endpoint.hostname, endpoint.port, 750);
  if (portOpen && await canReadCdpVersion(endpoint, 750)) {
    return { cdpUrl: endpoint.origin, started: false };
  }
  if (portOpen) {
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

function rejectKnownBadCdpEndpoint(endpoint, explicit) {
  if (!explicit) {
    return;
  }
  if (isLocalCdpHost(endpoint.hostname) && endpoint.port === 922) {
    throw new Error('Chrome CDP port 922 is almost certainly a typo. The managed Jailgun Chrome default is http://127.0.0.1:9224; omit --cdp-url or use that URL explicitly.');
  }
}

function isLocalCdpHost(hostname) {
  return hostname === '127.0.0.1' || hostname === 'localhost' || hostname === '::1';
}

async function canReadCdpVersion(endpoint, timeoutMs) {
  try {
    await fetchCdpVersion(endpoint, timeoutMs);
    return true;
  } catch {
    return false;
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
    for (const el of controls) {
      if (!visible(el) || disabled(el)) continue;
      const text = label(el);
      if (/\b(stop answering|stop generating|stop responding|stop thinking|stop)\b/i.test(text)) activeStop = true;
      if (/\b(copy response|good response|bad response|more actions|sources)\b/i.test(text)) finalActions += 1;
    }
    return { activeStop, finalActions };
  });
}

async function downloadCandidate(page, candidate, outputDir) {
  const downloadPromise = page.waitForEvent('download', { timeout: 120000 });
  const locator = page.locator('a,button,[role="button"],[download],[href]').nth(candidate.index);
  await locator.scrollIntoViewIfNeeded({ timeout: 5000 }).catch(() => undefined);
  await locator.click({ timeout: 120000 });
  const download = await downloadPromise;
  const suggested = normalizeTarName(download.suggestedFilename() || basename(candidate.href || '') || 'chatgpt-output.tar.gz');
  const path = join(outputDir, suggested);
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
        return style.visibility !== 'hidden' && style.display !== 'none' && rect.width >= 0 && rect.height >= 0;
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

function resolvePath(value) {
  return isAbsolute(value) ? value : resolve(process.cwd(), value);
}

function timestamp() {
  return new Date().toISOString();
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
  validateEnvelope({
    v: 1,
    type: 'hello',
    run_id: 'run-test',
    ts: timestamp(),
    payload: {},
  });
  await verifyReachableCdpEndpointIsReused();
  process.stdout.write('chrome-bridge self-test passed\n');
}

async function verifyReachableCdpEndpointIsReused() {
  let versionRequests = 0;
  const server = http.createServer((request, response) => {
    if (request.url === '/json/version') {
      versionRequests += 1;
      response.writeHead(200, { 'content-type': 'application/json' });
      response.end(JSON.stringify({
        Browser: 'Chrome/999.0.0.0',
        'Protocol-Version': '1.3',
      }));
      return;
    }
    response.writeHead(404, { 'content-type': 'text/plain' });
    response.end('not found');
  });

  await new Promise((resolvePromise, rejectPromise) => {
    server.once('error', rejectPromise);
    server.listen(0, '127.0.0.1', resolvePromise);
  });

  const tempRoot = await mkdtemp(join(tmpdir(), 'chrome-bridge-self-test-'));
  try {
    const address = server.address();
    if (!address || typeof address === 'string') {
      throw new Error('mock CDP server did not expose a port');
    }
    const cdpUrl = `http://127.0.0.1:${address.port}`;
    const result = await ensureManagedChromeRunning({
      cdpUrl,
      cdpUrlExplicit: true,
      profileDir: join(tempRoot, 'profile'),
      stateDir: join(tempRoot, 'state'),
      chromeExecutable: '/definitely/not-used',
      browserTimeoutMs: 2000,
      downloadsDir: join(tempRoot, 'downloads'),
      artifactsDir: join(tempRoot, 'artifacts'),
      sourceMode: DEFAULT_SOURCE_ARCHIVE_MODE,
      tarTargetName: '',
      submitDelaySeconds: 0,
      submitJitterSeconds: 0,
      tarWaitMinutes: 1,
    });

    if (result.started !== false) {
      throw new Error('reachable CDP endpoint should be reused instead of starting Chrome');
    }
    if (result.cdpUrl !== cdpUrl) {
      throw new Error(`unexpected CDP URL from reused endpoint: ${result.cdpUrl}`);
    }
    if (versionRequests === 0) {
      throw new Error('reachable CDP endpoint was not queried');
    }
  } finally {
    await rm(tempRoot, { recursive: true, force: true });
    await new Promise((resolvePromise) => {
      server.close(() => resolvePromise());
    });
  }
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
await bridge.run();

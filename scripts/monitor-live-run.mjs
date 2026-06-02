#!/usr/bin/env node
import { existsSync, statSync } from 'node:fs';
import { mkdir, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { chromium } from 'playwright-core';

const args = parseArgs(process.argv.slice(2));
const runId = required(args.runId ?? process.env.JAILGUN_RUN_ID, '--run-id');
const dashboardUrl = trimTrailingSlash(
  args.dashboardUrl ?? process.env.JAILGUN_DASHBOARD_URL ?? 'http://127.0.0.1:8787',
);
const artifactDir = resolve(
  args.artifactDir ?? process.env.JAILGUN_MONITOR_ARTIFACT_DIR ?? join('artifacts', 'live-runs', runId),
);
const expectedTabs = numberFrom(args.expectedTabs ?? process.env.JAILGUN_TABS, 7);
const expectedLoops = numberFrom(args.expectedLoops ?? process.env.JAILGUN_LOOPS, 0);
const timeoutMs = numberFrom(args.timeoutMs ?? process.env.JAILGUN_MONITOR_TIMEOUT_MS, 4 * 60 * 60 * 1000);
const pollMs = numberFrom(args.pollMs ?? process.env.JAILGUN_MONITOR_POLL_MS, 2500);
const maxDownloadStartLatencyMs = numberFrom(
  args.maxDownloadStartLatencyMs ?? process.env.JAILGUN_MAX_DOWNLOAD_START_LATENCY_MS,
  10_000,
);
const expectedRemoteHost = args.expectedRemoteHost ?? process.env.JAILGUN_REMOTE_HOST ?? '';
const expectedRemoteCommand = args.expectedRemoteCommand ?? process.env.JAILGUN_REMOTE_COMMAND ?? '';

await mkdir(artifactDir, { recursive: true });

const wsEvents = [];
const proof = {
  run_id: runId,
  dashboard_url: dashboardUrl,
  expected_tabs: expectedTabs,
  expected_loops: expectedLoops,
  observed_tabs: 0,
  tab_ids: [],
  batch_tabs: 0,
  loop_count: 0,
  planned_tabs: 0,
  loops_remaining: 0,
  loop_banner_visible: false,
  loop_banner_text: null,
  download_started: 0,
  download_start_within_10s: 0,
  download_receipts: 0,
  generation_stopped: 0,
  early_stops_succeeded: 0,
  early_stops_attempted: 0,
  tabs_closed: 0,
  deploy_successes: 0,
  remote_host_matches: 0,
  remote_command_matches: 0,
  remote_local_ci_passed: 0,
  ci_passed: 0,
  ci_skipped: 0,
  rate_limit_detections: 0,
  rate_limit_dismissed: 0,
  rate_limit_undismissed: 0,
  error_events: 0,
  fresh_downloads: 0,
  local_download_paths_unique: false,
  max_download_start_latency_ms: null,
  websocket_open: false,
  api_runs_open: false,
  receipts_open: false,
  dashboard_visible: false,
  screenshots: {
    ready: join(artifactDir, 'dashboard-ready.png'),
    final: join(artifactDir, 'dashboard-final.png'),
  },
  last_error: null,
  status: 'running',
  checked_at: new Date().toISOString(),
};

let browser;
let page;
let unsubscribe = () => undefined;

try {
  browser = await launchMonitorBrowser();
  page = await browser.newPage({ viewport: { width: 1440, height: 980 } });
  await page.goto(dashboardUrl, { waitUntil: 'domcontentloaded', timeout: 30_000 });
  await page.waitForSelector('main, #root', { timeout: 15_000 });
  proof.dashboard_visible = await page.locator('text=Jailgun').first().isVisible().catch(() => false);
  await page.screenshot({ path: proof.screenshots.ready, fullPage: true });

  unsubscribe = subscribeEvents(`${dashboardUrl.replace(/^http/i, 'ws')}/ws/events`, wsEvents, (error) => {
    proof.last_error = error.message;
  });

  const deadline = Date.now() + timeoutMs;
  let succeeded = false;
  while (Date.now() < deadline) {
    const sample = await sampleRunState();
    Object.assign(proof, sample, {
      websocket_open: proof.websocket_open || sample.websocket_open,
      dashboard_visible: proof.dashboard_visible || sample.dashboard_visible,
      checked_at: new Date().toISOString(),
    });
    await writeProof(proof);

    if (expectedLoops > 0 && proof.dashboard_visible && !proof.loop_banner_visible) {
      proof.status = 'failed';
      proof.last_error = 'loop banner missing from dashboard while loops were requested';
      await page.screenshot({ path: proof.screenshots.final, fullPage: true }).catch(() => undefined);
      await writeProof(proof);
      printFailure(proof.last_error, proof);
      process.exitCode = 1;
      break;
    }

    if (meetsSuccess(proof)) {
      proof.status = 'success';
      await page.goto(dashboardUrl, { waitUntil: 'networkidle', timeout: 30_000 }).catch(() => undefined);
      await page.screenshot({ path: proof.screenshots.final, fullPage: true });
      await writeProof(proof);
      printSuccess(proof);
      process.exitCode = 0;
      succeeded = true;
      break;
    }
    await delay(pollMs);
  }

  if (!succeeded) {
    proof.status = 'failed';
    proof.last_error = `timed out after ${timeoutMs}ms waiting for ${expectedTabs} planned tabs/downloads/deploys`;
    await page.screenshot({ path: proof.screenshots.final, fullPage: true }).catch(() => undefined);
    await writeProof(proof);
    printFailure(proof.last_error, proof);
    process.exitCode = 1;
  }
} catch (error) {
  proof.status = 'failed';
  proof.last_error = error instanceof Error ? error.message : String(error);
  if (page) {
    await page.screenshot({ path: proof.screenshots.final, fullPage: true }).catch(() => undefined);
  }
  await writeProof(proof);
  printFailure(proof.last_error, proof);
  process.exitCode = 1;
} finally {
  unsubscribe();
  await browser?.close().catch(() => undefined);
}

async function sampleRunState() {
  const events = wsEvents.slice();
  let runs = [];
  let receipts = [];
  let apiRunsOpen = false;
  let receiptsOpen = false;
  try {
    const response = await fetch(`${dashboardUrl}/api/runs`);
    apiRunsOpen = response.ok;
    if (response.ok) {
      runs = await response.json();
    }
  } catch (error) {
    proof.last_error = `GET /api/runs failed: ${messageOf(error)}`;
  }
  try {
    const response = await fetch(`${dashboardUrl}/api/receipts/${encodeURIComponent(runId)}`);
    receiptsOpen = response.ok;
    if (response.ok) {
      const body = await response.json();
      receipts = Array.isArray(body.receipts) ? body.receipts : [];
    }
  } catch (error) {
    proof.last_error = `GET /api/receipts/${runId} failed: ${messageOf(error)}`;
  }

  const run = Array.isArray(runs) ? runs.find((candidate) => candidate.run_id === runId) : null;
  const runStartedAt = firstEventTime(events, 'run-started') ?? (run?.started_at ? Date.parse(run.started_at) : null);
  const batchTabs = positiveInteger(
    run?.batch_tabs ??
    firstEventField(events, 'run-started', 'batch_tabs') ??
    firstEventField(events, 'run-started', 'tabs')
  ) ?? 0;
  const loopCount = positiveInteger(
    run?.loop_count ??
    firstEventField(events, 'run-started', 'loop_count')
  ) ?? 0;
  const plannedTabs = positiveInteger(
    run?.planned_tabs ??
    firstEventField(events, 'run-started', 'planned_tabs') ??
    firstEventField(events, 'run-started', 'tabs')
  ) ?? 0;
  const snapshotTabs = Array.isArray(run?.tabs) ? run.tabs.map((tab) => Number(tab.tab_id)) : [];
  const eventTabs = distinctTabs(events);
  const tabIds = sortedUnique([...snapshotTabs, ...eventTabs]);
  const downloadStartedEvents = events
    .filter((event) => event.run_id === runId && event.kind === 'download-started' && event.tab_id !== null);
  const downloadStartedTabs = sortedUnique(downloadStartedEvents.map((event) => Number(event.tab_id)));
  const tarDiscoveredEvents = events
    .filter((event) => event.run_id === runId && event.kind === 'tar-discovered' && event.tab_id !== null);
  const downloadStartLatencies = downloadStartedTabs
    .map((tabId) => downloadStartLatencyForTab(tarDiscoveredEvents, downloadStartedEvents, tabId))
    .filter((value) => value !== null);
  const downloadStartWithinLimitTabs = sortedUnique(downloadStartedTabs.filter((tabId) => {
    const latency = downloadStartLatencyForTab(tarDiscoveredEvents, downloadStartedEvents, tabId);
    return latency !== null && latency <= maxDownloadStartLatencyMs;
  }));
  const downloadReceiptEvents = events
    .filter((event) => event.run_id === runId && event.kind === 'download-receipt' && event.tab_id !== null);
  const downloadTabs = sortedUnique(events
    .filter((event) => event.run_id === runId && event.kind === 'download-receipt' && event.tab_id !== null)
    .map((event) => Number(event.tab_id)));
  const generationStoppedTabs = sortedUnique(events
    .filter((event) => event.run_id === runId && event.kind === 'generation-stopped' && event.tab_id !== null)
    .map((event) => Number(event.tab_id)));
  const earlyStopEvents = events.filter((event) =>
    event.run_id === runId &&
    event.kind === 'generation-stopped' &&
    event.tab_id !== null &&
    (event.fields?.phase === 'pre-download' || event.fields?.phase === 'post-download'));
  const earlyStopAttemptedTabs = sortedUnique(earlyStopEvents.map((event) => Number(event.tab_id)));
  const earlyStopSucceededTabs = sortedUnique(earlyStopEvents
    .filter((event) => earlyStopMethodIsSuccess(event.fields?.method ?? ''))
    .map((event) => Number(event.tab_id)));
  const tabClosedTabs = sortedUnique(events
    .filter((event) => event.run_id === runId && event.kind === 'tab-closed' && event.tab_id !== null)
    .map((event) => Number(event.tab_id)));
  const freshDownloads = downloadReceiptEvents.map((event) => checkFreshDownload(event, runStartedAt));
  const freshDownloadTabs = sortedUnique(freshDownloads.filter((item) => item.ok).map((item) => item.tab_id));
  const localDownloadPaths = freshDownloads.map((item) => item.local_path).filter(Boolean);
  const deployTabs = sortedUnique(events
    .filter((event) => {
      if (event.run_id !== runId || event.kind !== 'deploy-finished' || event.severity === 'error') {
        return false;
      }
      const outcome = event.fields?.outcome ?? '';
      return outcome === 'succeeded' || outcome === 'succeeded-ci-skipped' || outcome === 'dry-run-staged';
    })
    .map((event) => Number(event.tab_id)));
  const successfulDeployEvents = events
    .filter((event) => event.run_id === runId && event.kind === 'deploy-finished' && event.severity !== 'error');
  const remoteHostTabs = sortedUnique(successfulDeployEvents
    .filter((event) => !expectedRemoteHost || event.fields?.remote_host === expectedRemoteHost)
    .map((event) => Number(event.tab_id)));
  const remoteCommandTabs = sortedUnique(successfulDeployEvents
    .filter((event) => !expectedRemoteCommand || event.fields?.remote_command === expectedRemoteCommand)
    .map((event) => Number(event.tab_id)));
  const remoteLocalCiPassedTabs = sortedUnique(successfulDeployEvents
    .filter((event) => remoteLocalCiPassed(event))
    .map((event) => Number(event.tab_id)));
  const ciPassedTabs = sortedUnique(successfulDeployEvents
    .filter((event) => event.fields?.ci_state === 'passed')
    .map((event) => Number(event.tab_id)));
  const ciSkippedTabs = sortedUnique(successfulDeployEvents
    .filter((event) => event.fields?.ci_state === 'skipped')
    .map((event) => Number(event.tab_id)));
  const rateLimitEvents = events.filter((event) => event.run_id === runId && event.kind === 'rate-limit-detected');
  const rateLimitDismissedEvents = rateLimitEvents.filter((event) => truthyField(event.fields?.dismissed));
  const rateLimitUndismissedEvents = rateLimitEvents.filter((event) => !truthyField(event.fields?.dismissed));
  const errorEvents = events.filter((event) => event.run_id === runId && event.severity === 'error');
  const dashboardVisible = page
    ? await page.locator(`text=${runId}`).first().isVisible().catch(() => false)
    : false;
  const loopBanner = page ? page.locator('[aria-label="looping status"]').first() : null;
  const loopBannerVisible = loopBanner ? await loopBanner.isVisible().catch(() => false) : false;
  const loopBannerText = loopBanner ? await loopBanner.textContent().catch(() => null) : null;
  const loopsRemaining = calculateLoopsRemaining(loopCount, batchTabs, tabIds.length);

  return {
    api_runs_open: apiRunsOpen,
    receipts_open: receiptsOpen,
    websocket_open: events.length > 0,
    dashboard_visible: dashboardVisible,
    batch_tabs: batchTabs,
    loop_count: loopCount,
    planned_tabs: plannedTabs,
    loops_remaining: loopsRemaining,
    loop_banner_visible: loopBannerVisible,
    loop_banner_text: loopBannerText,
    observed_tabs: tabIds.length,
    tab_ids: tabIds,
    download_started: downloadStartedTabs.length,
    download_started_tab_ids: downloadStartedTabs,
    download_start_within_10s: downloadStartWithinLimitTabs.length,
    download_start_within_10s_tab_ids: downloadStartWithinLimitTabs,
    max_download_start_latency_ms: downloadStartLatencies.length > 0 ? Math.max(...downloadStartLatencies) : null,
    download_receipts: downloadTabs.length,
    download_tab_ids: downloadTabs,
    generation_stopped: generationStoppedTabs.length,
    generation_stopped_tab_ids: generationStoppedTabs,
    early_stops_succeeded: earlyStopSucceededTabs.length,
    early_stops_succeeded_tab_ids: earlyStopSucceededTabs,
    early_stops_attempted: earlyStopAttemptedTabs.length,
    early_stops_attempted_tab_ids: earlyStopAttemptedTabs,
    tabs_closed: tabClosedTabs.length,
    tab_closed_ids: tabClosedTabs,
    fresh_downloads: freshDownloadTabs.length,
    fresh_download_tab_ids: freshDownloadTabs,
    fresh_download_failures: freshDownloads.filter((item) => !item.ok),
    local_download_paths_unique: new Set(localDownloadPaths).size === localDownloadPaths.length,
    deploy_successes: deployTabs.length,
    deploy_tab_ids: deployTabs,
    remote_host_matches: remoteHostTabs.length,
    remote_host_tab_ids: remoteHostTabs,
    remote_command_matches: remoteCommandTabs.length,
    remote_command_tab_ids: remoteCommandTabs,
    remote_local_ci_passed: remoteLocalCiPassedTabs.length,
    remote_local_ci_passed_tab_ids: remoteLocalCiPassedTabs,
    ci_passed: ciPassedTabs.length,
    ci_passed_tab_ids: ciPassedTabs,
    ci_skipped: ciSkippedTabs.length,
    ci_skipped_tab_ids: ciSkippedTabs,
    rate_limit_detections: rateLimitEvents.length,
    rate_limit_dismissed: rateLimitDismissedEvents.length,
    rate_limit_undismissed: rateLimitUndismissedEvents.length,
    rate_limit_undismissed_messages: rateLimitUndismissedEvents.map((event) => ({
      tab_id: event.tab_id,
      message: event.message,
      fields: event.fields,
    })),
    error_events: errorEvents.length,
    error_event_messages: errorEvents.map((event) => ({
      tab_id: event.tab_id,
      kind: event.kind,
      message: event.message,
      fields: event.fields,
    })),
    receipt_count: receipts.length,
  };
}

function subscribeEvents(url, events, onError) {
  if (typeof WebSocket === 'undefined') {
    onError(new Error('global WebSocket is unavailable in this Node runtime'));
    return () => undefined;
  }
  const socket = new WebSocket(url);
  socket.onopen = () => {
    proof.websocket_open = true;
  };
  socket.onmessage = (message) => {
    try {
      const event = JSON.parse(message.data);
      if (event.run_id === runId) {
        events.push(event);
      }
    } catch (error) {
      onError(error instanceof Error ? error : new Error(String(error)));
    }
  };
  socket.onerror = () => onError(new Error(`WebSocket error at ${url}`));
  return () => socket.close();
}

async function launchMonitorBrowser() {
  const executablePath = process.env.JAILGUN_MONITOR_CHROME_EXECUTABLE || firstExisting([
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/Applications/Google Chrome Beta.app/Contents/MacOS/Google Chrome Beta',
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
  ]);
  const common = {
    headless: true,
    args: ['--no-first-run', '--no-default-browser-check'],
  };
  if (executablePath) {
    return chromium.launch({ ...common, executablePath });
  }
  return chromium.launch({ ...common, channel: 'chrome' });
}

function meetsSuccess(value) {
  const loopsOk = expectedLoops === 0
    ? !value.loop_banner_visible
    : value.loop_banner_visible && value.loop_banner_text?.includes(`${value.loops_remaining} left`) && value.loops_remaining === 0;
  return value.api_runs_open &&
    value.receipts_open &&
    value.websocket_open &&
    value.dashboard_visible &&
    value.observed_tabs === expectedTabs &&
    value.planned_tabs === expectedTabs &&
    hasExpectedIds(value.tab_ids) &&
    value.download_started === expectedTabs &&
    value.download_start_within_10s === expectedTabs &&
    value.download_receipts === expectedTabs &&
    value.generation_stopped === expectedTabs &&
    value.tabs_closed === expectedTabs &&
    value.fresh_downloads === expectedTabs &&
    value.local_download_paths_unique &&
    value.deploy_successes === expectedTabs &&
    value.remote_host_matches === expectedTabs &&
    value.remote_command_matches === expectedTabs &&
    value.remote_local_ci_passed === expectedTabs &&
    value.rate_limit_undismissed === 0 &&
    value.error_events === 0 &&
    loopsOk &&
    screenshotExists(value.screenshots.ready);
}

function hasExpectedIds(ids) {
  const set = new Set(ids);
  for (let index = 1; index <= expectedTabs; index += 1) {
    if (!set.has(index)) {
      return false;
    }
  }
  return true;
}

async function writeProof(value) {
  await writeFile(join(artifactDir, 'monitor-proof.json'), `${JSON.stringify(value, null, 2)}\n`);
}

function printSuccess(value) {
  process.stdout.write([
    'SUCCESS: live dashboard monitor verified',
    `run id: ${value.run_id}`,
    `dashboard URL: ${value.dashboard_url}`,
    `observed tabs: ${value.observed_tabs}`,
    `batch tabs: ${value.batch_tabs}`,
    `loop count: ${value.loop_count}`,
    `planned tabs: ${value.planned_tabs}`,
    `loops remaining: ${value.loops_remaining}`,
    `download started: ${value.download_started}`,
    `download start <=10s: ${value.download_start_within_10s}`,
    `download receipts: ${value.download_receipts}`,
    `generation stopped: ${value.generation_stopped}`,
    `tabs closed: ${value.tabs_closed}`,
    `fresh downloads: ${value.fresh_downloads}`,
    `deploy successes: ${value.deploy_successes}`,
    `remote host matches: ${value.remote_host_matches}`,
    `remote command matches: ${value.remote_command_matches}`,
    `remote local CI passed: ${value.remote_local_ci_passed}`,
    `GitHub CI passed: ${value.ci_passed}`,
    `GitHub CI skipped: ${value.ci_skipped}`,
    `rate limit detections: ${value.rate_limit_detections}`,
    `rate limit dismissed: ${value.rate_limit_dismissed}`,
    `rate limit undismissed: ${value.rate_limit_undismissed}`,
    `error events: ${value.error_events}`,
    `proof: ${join(artifactDir, 'monitor-proof.json')}`,
  ].join('\n') + '\n');
}

function printFailure(reason, value) {
  process.stderr.write([
    `FAILED: ${reason}`,
    `checked endpoint: ${value.dashboard_url}/api/runs`,
    `checked path: ${join(artifactDir, 'monitor-proof.json')}`,
    `next action: inspect ${value.screenshots.final} and ${join(artifactDir, 'monitor-proof.json')}`,
  ].join('\n') + '\n');
}

function distinctTabs(events) {
  return sortedUnique(events
    .filter((event) => event.run_id === runId && event.tab_id !== null && event.tab_id !== undefined)
    .map((event) => Number(event.tab_id)));
}

function earlyStopMethodIsSuccess(method) {
  if (!method) return false;
  return !method.startsWith('not-active')
    && !method.startsWith('not-run')
    && !method.startsWith('shutdown');
}

function remoteLocalCiPassed(event) {
  const fields = event.fields ?? {};
  if (expectedRemoteHost && fields.remote_host !== expectedRemoteHost) {
    return false;
  }
  if (expectedRemoteCommand && fields.remote_command !== expectedRemoteCommand) {
    return false;
  }
  if (fields.exit_code !== '0') {
    return false;
  }
  const logTail = String(fields.log_tail ?? '');
  if (!expectedRemoteCommand.includes('ci-fast-push')) {
    return true;
  }
  return logTail.includes('ci-fast-push: jekko-fast passed') &&
    /cargo test:\s+\d+ passed/.test(logTail) &&
    logTail.includes('DONE: pre=') &&
    logTail.includes(' post=');
}

function firstEventTime(events, kind) {
  const event = events.find((candidate) => candidate.run_id === runId && candidate.kind === kind);
  return event ? Date.parse(event.timestamp) : null;
}

function firstEventField(events, kind, field) {
  const event = events.find((candidate) => candidate.run_id === runId && candidate.kind === kind);
  const value = event?.fields?.[field];
  return value === undefined ? null : value;
}

function downloadStartLatencyForTab(tarEvents, downloadStartedEvents, tabId) {
  const tar = tarEvents.find((event) => Number(event.tab_id) === tabId);
  const started = downloadStartedEvents.find((event) => Number(event.tab_id) === tabId);
  if (!tar || !started) {
    return null;
  }
  const tarTime = Date.parse(tar.timestamp);
  const startTime = Date.parse(started.timestamp);
  if (!Number.isFinite(tarTime) || !Number.isFinite(startTime)) {
    return null;
  }
  return Math.max(0, startTime - tarTime);
}

function checkFreshDownload(event, runStartedAt) {
  const tabId = Number(event.tab_id);
  const localPath = event.fields?.local_path ?? '';
  const expectedSize = Number(event.fields?.size_bytes ?? 0);
  const expectedTabDir = `tab-${String(tabId).padStart(2, '0')}`;
  const base = {
    tab_id: tabId,
    local_path: localPath,
    reason: '',
    ok: false,
  };
  if (!localPath) {
    return { ...base, reason: 'missing-local-path' };
  }
  if (!localPath.endsWith('.tar.gz')) {
    return { ...base, reason: 'not-tar-gz' };
  }
  if (!localPath.includes(expectedTabDir)) {
    return { ...base, reason: `path-missing-${expectedTabDir}` };
  }
  let fileStat;
  try {
    fileStat = statSync(localPath);
  } catch {
    return { ...base, reason: 'file-missing' };
  }
  if (!fileStat.isFile()) {
    return { ...base, reason: 'not-file' };
  }
  if (fileStat.size <= 0) {
    return { ...base, reason: 'empty-file' };
  }
  if (Number.isFinite(expectedSize) && expectedSize > 0 && fileStat.size !== expectedSize) {
    return { ...base, reason: `size-mismatch:${fileStat.size}:${expectedSize}` };
  }
  if (runStartedAt && Number.isFinite(runStartedAt) && fileStat.mtimeMs + 1000 < runStartedAt) {
    return { ...base, reason: 'mtime-before-run' };
  }
  return { ...base, size_bytes: fileStat.size, mtime: fileStat.mtime.toISOString(), ok: true };
}

function calculateLoopsRemaining(loopCount, batchTabs, observedTabs) {
  if (loopCount <= 0 || batchTabs <= 0) {
    return 0;
  }
  const batchesStarted = Math.max(0, Math.ceil(observedTabs / batchTabs) - 1);
  return Math.max(0, loopCount - batchesStarted);
}

function positiveInteger(value) {
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : null;
}

function sortedUnique(values) {
  return [...new Set(values.filter((value) => Number.isInteger(value) && value > 0))]
    .sort((left, right) => left - right);
}

function truthyField(value) {
  return value === true || String(value).toLowerCase() === 'true';
}

function parseArgs(values) {
  const parsed = {};
  for (let index = 0; index < values.length; index += 1) {
    const key = values[index];
    if (!key.startsWith('--')) {
      throw new Error(`unexpected argument: ${key}`);
    }
    const name = key.slice(2).replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());
    const value = values[index + 1];
    if (value === undefined || value.startsWith('--')) {
      throw new Error(`${key} requires a value`);
    }
    parsed[name] = value;
    index += 1;
  }
  return parsed;
}

function numberFrom(value, fallback) {
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

function required(value, label) {
  if (typeof value !== 'string' || value.trim() === '') {
    throw new Error(`${label} is required`);
  }
  return value.trim();
}

function trimTrailingSlash(value) {
  return String(value).replace(/\/+$/, '');
}

function firstExisting(paths) {
  return paths.find((path) => existsSync(path));
}

function screenshotExists(path) {
  try {
    return statSync(path).size > 0;
  } catch {
    return false;
  }
}

function delay(ms) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, ms));
}

function messageOf(error) {
  return error instanceof Error ? error.message : String(error);
}

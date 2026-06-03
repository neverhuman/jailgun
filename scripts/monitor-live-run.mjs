#!/usr/bin/env node
import { existsSync, statSync } from 'node:fs';
import { mkdir, writeFile } from 'node:fs/promises';
import { delimiter, join, resolve } from 'node:path';
import { chromium } from 'playwright-core';

if (process.argv.includes('--help') || process.argv.includes('-h')) {
  printHelp();
  process.exit(0);
}

if (process.argv.includes('--self-test-profile-distribution')) {
  runProfileDistributionSelfTest();
  process.exit(0);
}

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
const expectedProfileDirs = pathListFrom(
  args.expectedProfileDir ?? args.expectedProfileDirs ?? process.env.JAILGUN_EXPECTED_PROFILE_DIRS,
);
const expectedCdpUrls = urlListFrom(
  args.expectedCdpUrl ?? args.expectedCdpUrls ?? process.env.JAILGUN_EXPECTED_CDP_URLS,
);
const expectedBrowserSlots = expectedBrowserSlotsFor(expectedProfileDirs, expectedCdpUrls);
const expectDeploy = booleanFrom(
  args.expectDeploy ?? process.env.JAILGUN_MONITOR_EXPECT_DEPLOY,
  true,
);

if (
  expectedProfileDirs.length > 0 &&
  expectedCdpUrls.length > 0 &&
  expectedProfileDirs.length !== expectedCdpUrls.length
) {
  throw new Error('--expected-profile-dir and --expected-cdp-url counts must match');
}

await mkdir(artifactDir, { recursive: true });

const wsEvents = [];
const proof = {
  run_id: runId,
  dashboard_url: dashboardUrl,
  expected_tabs: expectedTabs,
  expected_loops: expectedLoops,
  expect_deploy: expectDeploy,
  expected_profile_dirs: expectedProfileDirs,
  expected_browser_slots: expectedBrowserSlots,
  expected_cdp_urls: expectedCdpUrls,
  observed_tabs: 0,
  tab_ids: [],
  tab_opened_count: 0,
  tab_opened_ids: [],
  browser_profile_dirs: {},
  browser_profile_expected_counts: {},
  browser_profile_distribution_ok: expectedProfileDirs.length === 0,
  browser_slots: {},
  browser_slot_expected_counts: {},
  browser_slot_distribution_ok: expectedBrowserSlots.length === 0,
  browser_cdp_urls: {},
  browser_cdp_expected_counts: {},
  browser_cdp_distribution_ok: expectedCdpUrls.length === 0,
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
  fresh_source_clones: 0,
  files_changed: 0,
  additions: 0,
  deletions: 0,
  local_tests_passed: 0,
  remote_tests_passed: 0,
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
  receipt_count: 0,
  max_download_start_latency_ms: null,
  websocket_open: false,
  api_runs_open: false,
  receipts_open: false,
  api_receipt_count: 0,
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
  const browserDistribution = analyzeBrowserDistribution(events, {
    runId,
    expectedTabs,
    expectedProfileDirs,
    expectedBrowserSlots,
    expectedCdpUrls,
  });
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
  const sourceCloneTabs = sortedUnique(events
    .filter((event) =>
      event.run_id === runId &&
      event.tab_id !== null &&
      event.tab_id !== undefined &&
      event.kind === 'browser-log' &&
      event.fields?.phase === 'source-upload' &&
      truthyField(event.fields?.fresh_source_clone) &&
      event.fields?.clone_dir)
    .map((event) => Number(event.tab_id)));
  const changeStats = aggregateDeployStats(successfulDeployEvents);
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
    ...browserDistribution,
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
    fresh_source_clones: sourceCloneTabs.length,
    fresh_source_clone_tab_ids: sourceCloneTabs,
    files_changed: changeStats.filesChanged,
    additions: changeStats.additions,
    deletions: changeStats.deletions,
    local_tests_passed: changeStats.localTestsPassed,
    remote_tests_passed: changeStats.remoteTestsPassed,
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
    api_receipt_count: receipts.length,
    receipt_count: receipts.length + downloadTabs.length,
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
  const deployOk = expectDeploy
    ? value.deploy_successes === expectedTabs &&
      value.remote_host_matches === expectedTabs &&
      value.remote_command_matches === expectedTabs &&
      value.remote_local_ci_passed === expectedTabs
    : value.deploy_successes === 0 &&
      value.remote_host_matches === 0 &&
      value.remote_command_matches === 0 &&
      value.remote_local_ci_passed === 0;
  return value.api_runs_open &&
    value.receipts_open &&
    value.receipt_count > 0 &&
    value.websocket_open &&
    value.dashboard_visible &&
    value.observed_tabs === expectedTabs &&
    value.planned_tabs === expectedTabs &&
    hasExpectedIds(value.tab_ids) &&
    value.tab_opened_count === expectedTabs &&
    value.browser_profile_distribution_ok &&
    value.browser_slot_distribution_ok &&
    value.browser_cdp_distribution_ok &&
    value.download_started === expectedTabs &&
    value.download_start_within_10s === expectedTabs &&
    value.download_receipts === expectedTabs &&
    value.generation_stopped === expectedTabs &&
    value.tabs_closed === expectedTabs &&
    value.fresh_downloads === expectedTabs &&
    value.local_download_paths_unique &&
    deployOk &&
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
    `tab-opened events: ${value.tab_opened_count}`,
    `profile dirs: ${JSON.stringify(value.browser_profile_dirs)}`,
    `browser slots: ${JSON.stringify(value.browser_slots)}`,
    `CDP URLs: ${JSON.stringify(value.browser_cdp_urls)}`,
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
    `receipt count: ${value.receipt_count}`,
    `API receipt count: ${value.api_receipt_count}`,
    `expect deploy: ${value.expect_deploy}`,
    `deploy successes: ${value.deploy_successes}`,
    `fresh source clones: ${value.fresh_source_clones}`,
    `files changed: ${value.files_changed}`,
    `additions: ${value.additions}`,
    `deletions: ${value.deletions}`,
    `local tests passed: ${value.local_tests_passed}`,
    `remote tests passed: ${value.remote_tests_passed}`,
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

function analyzeBrowserDistribution(events, {
  runId: expectedRunId,
  expectedTabs: plannedTabCount,
  expectedProfileDirs: profileDirs,
  expectedBrowserSlots: browserSlots,
  expectedCdpUrls: cdpUrls,
}) {
  const tabOpenedEvents = events.filter((event) =>
    event.run_id === expectedRunId &&
    event.kind === 'tab-opened' &&
    event.tab_id !== null &&
    event.tab_id !== undefined);
  const tabOpenedIds = sortedUnique(tabOpenedEvents.map((event) => Number(event.tab_id)));
  const profileCounts = countEventField(tabOpenedEvents, (event) => event.fields?.browser_profile_dir);
  const slotCounts = countEventField(tabOpenedEvents, (event) => {
    const value = event.fields?.browser_slot;
    return value === undefined || value === null || value === '' ? '' : String(value);
  });
  const cdpCounts = countEventField(tabOpenedEvents, (event) => {
    const value = event.fields?.cdp_url;
    return value ? trimTrailingSlash(value) : '';
  });
  const expectedProfileCounts = expectedRoundRobinCounts(plannedTabCount, profileDirs);
  const expectedSlotCounts = expectedRoundRobinCounts(plannedTabCount, browserSlots);
  const expectedCdpCounts = expectedRoundRobinCounts(plannedTabCount, cdpUrls);
  return {
    tab_opened_count: tabOpenedIds.length,
    tab_opened_ids: tabOpenedIds,
    browser_profile_dirs: profileCounts,
    browser_profile_expected_counts: expectedProfileCounts,
    browser_profile_distribution_ok: profileDirs.length === 0 ||
      distributionMatches(profileCounts, expectedProfileCounts),
    browser_slots: slotCounts,
    browser_slot_expected_counts: expectedSlotCounts,
    browser_slot_distribution_ok: browserSlots.length === 0 ||
      distributionMatches(slotCounts, expectedSlotCounts),
    browser_cdp_urls: cdpCounts,
    browser_cdp_expected_counts: expectedCdpCounts,
    browser_cdp_distribution_ok: cdpUrls.length === 0 ||
      distributionMatches(cdpCounts, expectedCdpCounts),
  };
}

function countEventField(events, fieldFn) {
  const counts = {};
  for (const event of events) {
    const value = fieldFn(event);
    if (!value) {
      continue;
    }
    counts[value] = (counts[value] ?? 0) + 1;
  }
  return counts;
}

function expectedRoundRobinCounts(total, values) {
  const counts = {};
  for (const value of values) {
    counts[value] = 0;
  }
  if (values.length === 0) {
    return counts;
  }
  for (let index = 0; index < total; index += 1) {
    counts[values[index % values.length]] += 1;
  }
  return counts;
}

function distributionMatches(observed, expected) {
  const observedKeys = Object.keys(observed).sort();
  const expectedKeys = Object.keys(expected).sort();
  if (observedKeys.length !== expectedKeys.length) {
    return false;
  }
  for (let index = 0; index < expectedKeys.length; index += 1) {
    if (observedKeys[index] !== expectedKeys[index]) {
      return false;
    }
  }
  return expectedKeys.every((key) => observed[key] === expected[key]);
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

function aggregateDeployStats(events) {
  return events.reduce((acc, event) => {
    const fields = event.fields ?? {};
    const shortstat = parseShortstat(fields.shortstat);
    acc.filesChanged += numberFromField(fields.files_changed) ?? shortstat.filesChanged ?? 0;
    acc.additions += numberFromField(fields.additions) ?? shortstat.additions ?? 0;
    acc.deletions += numberFromField(fields.deletions) ?? shortstat.deletions ?? 0;
    acc.localTestsPassed += numberFromField(fields.local_tests_passed) ?? parseLogTestCount(fields.log_tail) ?? 0;
    acc.remoteTestsPassed += numberFromField(fields.remote_tests_passed) ?? parseLogTestCount(fields.log_tail) ?? 0;
    return acc;
  }, {
    filesChanged: 0,
    additions: 0,
    deletions: 0,
    localTestsPassed: 0,
    remoteTestsPassed: 0,
  });
}

function parseShortstat(value) {
  const text = String(value ?? '');
  return {
    filesChanged: numberBefore(text, /\bfiles? changed\b/),
    additions: numberBefore(text, /\b(?:insertions?|additions?)\(\+\)/),
    deletions: numberBefore(text, /\bdeletions?\(-\)/),
  };
}

function numberBefore(text, pattern) {
  const match = text.match(new RegExp(`(\\d+)\\s+${pattern.source}`, pattern.flags));
  return match ? Number(match[1]) : null;
}

function numberFromField(value) {
  if (value === undefined || value === null || value === '') {
    return null;
  }
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : null;
}

function parseLogTestCount(value) {
  const text = String(value ?? '');
  const matches = [...text.matchAll(/(?:cargo test|npm test|vitest|tests?)\s*:\s*(\d+)\s+passed/gi)];
  if (matches.length === 0) {
    return null;
  }
  return matches.reduce((sum, match) => sum + Number(match[1]), 0);
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
    if (parsed[name] === undefined) {
      parsed[name] = value;
    } else if (Array.isArray(parsed[name])) {
      parsed[name].push(value);
    } else {
      parsed[name] = [parsed[name], value];
    }
    index += 1;
  }
  return parsed;
}

function numberFrom(value, fallback) {
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

function booleanFrom(value, fallback) {
  if (value === undefined || value === null || value === '') {
    return fallback;
  }
  const text = String(value).trim().toLowerCase();
  if (['1', 'true', 'yes', 'y'].includes(text)) {
    return true;
  }
  if (['0', 'false', 'no', 'n'].includes(text)) {
    return false;
  }
  throw new Error(`expected boolean value, got ${value}`);
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

function pathListFrom(value) {
  return listFrom(value, { splitOnDelimiter: true })
    .map((item) => resolveTildePath(item));
}

function urlListFrom(value) {
  return listFrom(value, { splitOnComma: true })
    .map((item) => trimTrailingSlash(item));
}

function listFrom(value, { splitOnDelimiter = false, splitOnComma = false } = {}) {
  if (value === undefined || value === null || value === '') {
    return [];
  }
  const rawValues = Array.isArray(value) ? value : [value];
  const splitPattern = splitOnDelimiter
    ? delimiter
    : (splitOnComma ? ',' : null);
  return rawValues
    .flatMap((item) => splitPattern ? String(item).split(splitPattern) : [String(item)])
    .map((item) => item.trim())
    .filter(Boolean);
}

function resolveTildePath(value) {
  const text = String(value);
  if (text === '~') {
    return resolve(process.env.HOME ?? '.');
  }
  if (text.startsWith('~/')) {
    return resolve(process.env.HOME ?? '.', text.slice(2));
  }
  return resolve(text);
}

function expectedBrowserSlotsFor(profileDirs, cdpUrls) {
  const count = Math.max(profileDirs.length, cdpUrls.length);
  return Array.from({ length: count }, (_, index) => String(index + 1));
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

function printHelp() {
  process.stdout.write(`Usage: node scripts/monitor-live-run.mjs --run-id ID [options]

Required:
  --run-id ID

Common options:
  --dashboard-url URL
  --artifact-dir DIR
  --expected-tabs N
  --expected-loops N
  --expect-deploy true|false
  --expected-remote-host HOST
  --expected-remote-command COMMAND
  --expected-profile-dir DIR       repeat for each managed browser profile
  --expected-cdp-url URL           repeat for each managed browser CDP URL
  --max-download-start-latency-ms N
  --timeout-ms N
  --poll-ms N

Self-test:
  --self-test-profile-distribution
`);
}

function runProfileDistributionSelfTest() {
  const testRunId = 'monitor-profile-distribution-self-test';
  const profileDirs = ['/tmp/jailgun-profile-a', '/tmp/jailgun-profile-b'].map((item) => resolve(item));
  const cdpUrls = ['http://127.0.0.1:9224', 'http://127.0.0.1:9225'];
  for (const expectedTabs of [2, 7, 30, 40]) {
    const events = profileDistributionFixture(testRunId, expectedTabs, profileDirs, cdpUrls);
    const result = analyzeBrowserDistribution(events, {
      runId: testRunId,
      expectedTabs,
      expectedProfileDirs: profileDirs,
      expectedBrowserSlots: ['1', '2'],
      expectedCdpUrls: cdpUrls,
    });
    assertSelfTest(result.tab_opened_count === expectedTabs, `tab-opened count failed for ${expectedTabs}`);
    assertSelfTest(result.browser_profile_distribution_ok, `profile distribution failed for ${expectedTabs}`);
    assertSelfTest(result.browser_slot_distribution_ok, `slot distribution failed for ${expectedTabs}`);
    assertSelfTest(result.browser_cdp_distribution_ok, `CDP distribution failed for ${expectedTabs}`);
  }

  const wrongCdp = profileDistributionFixture(testRunId, 2, profileDirs, cdpUrls);
  wrongCdp[1].fields.cdp_url = 'http://127.0.0.1:9333';
  const wrongCdpResult = analyzeBrowserDistribution(wrongCdp, {
    runId: testRunId,
    expectedTabs: 2,
    expectedProfileDirs: profileDirs,
    expectedBrowserSlots: ['1', '2'],
    expectedCdpUrls: cdpUrls,
  });
  assertSelfTest(!wrongCdpResult.browser_cdp_distribution_ok, 'wrong CDP URL should fail distribution');

  const missingProfile = profileDistributionFixture(testRunId, 2, profileDirs, cdpUrls);
  delete missingProfile[0].fields.browser_profile_dir;
  const missingProfileResult = analyzeBrowserDistribution(missingProfile, {
    runId: testRunId,
    expectedTabs: 2,
    expectedProfileDirs: profileDirs,
    expectedBrowserSlots: ['1', '2'],
    expectedCdpUrls: cdpUrls,
  });
  assertSelfTest(!missingProfileResult.browser_profile_distribution_ok, 'missing profile dir should fail distribution');

  process.stdout.write('SUCCESS: monitor profile distribution self-test passed\n');
}

function profileDistributionFixture(runId, expectedTabs, profileDirs, cdpUrls) {
  return Array.from({ length: expectedTabs }, (_, index) => {
    const slotIndex = index % profileDirs.length;
    return {
      run_id: runId,
      kind: 'tab-opened',
      tab_id: index + 1,
      fields: {
        browser_profile_dir: profileDirs[slotIndex],
        browser_slot: String(slotIndex + 1),
        cdp_url: cdpUrls[slotIndex],
      },
    };
  });
}

function assertSelfTest(value, message) {
  if (!value) {
    throw new Error(message);
  }
}

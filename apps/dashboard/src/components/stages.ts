import type { JailgunEvent, RunSnapshot, TabSnapshot } from '../types';

export type StageStatus = 'pending' | 'active' | 'done' | 'failed';
export type StageKey = 'polling' | 'tar' | 'upload' | 'ci' | 'outcome';

export interface StageState {
  key: StageKey;
  label: string;
  status: StageStatus;
  detail: string;
}

const SUCCESS_OUTCOMES = new Set(['succeeded', 'succeeded-ci-skipped', 'done', 'validated', 'ok', 'success']);
const FAILURE_OUTCOMES = new Set([
  'failed',
  'failed-hard',
  'failed-preserved',
  'command-fail',
  'ci-fail',
  'timed-out',
  'upload-sha-mismatch',
  'error'
]);
const POLLING_STATUSES = new Set([
  'opening',
  'submitted',
  'generating',
  'tar-discovered',
  'waiting-for-tar',
  'downloading',
  'active'
]);
const UPLOAD_DONE_STATES = new Set([
  'upload-verified',
  'running',
  'unpacking',
  'command-running',
  'remote-job-launched',
  'done',
  'validated',
  'succeeded',
  'succeeded-ci-failed',
  'succeeded-ci-skipped'
]);
const CI_RUNNING_STATES = new Set(['running', 'unpacking', 'command-running', 'remote-job-launched']);
const CI_DONE_STATES = new Set([
  'done',
  'validated',
  'succeeded',
  'succeeded-ci-failed',
  'succeeded-ci-skipped',
  'success'
]);

export function deriveStages(tab: TabSnapshot): StageState[] {
  const status = (tab.status ?? '').toLowerCase();
  const deploy = (tab.deploy_status ?? '').toLowerCase();
  const closed = status === 'closed';
  const error = status === 'error' || deploy === 'error' || FAILURE_OUTCOMES.has(deploy);
  const tarCaptured = Boolean(tab.archive_sha256) || status === 'downloaded' || status === 'closed' || deploy !== 'pending';

  // Stage 1 — polling
  let pollingStatus: StageStatus;
  if (error && !tab.archive_sha256) {
    pollingStatus = 'failed';
  } else if (tarCaptured) {
    pollingStatus = 'done';
  } else if (POLLING_STATUSES.has(status) || status === '' || status === 'pending') {
    pollingStatus = 'active';
  } else {
    pollingStatus = 'active';
  }

  // Stage 2 — tar captured
  const tarStatus: StageStatus = tarCaptured ? 'done' : pollingStatus === 'failed' ? 'failed' : 'pending';

  // Stage 3 — upload to remote host
  let uploadStatus: StageStatus;
  if (deploy === 'upload-sha-mismatch') {
    uploadStatus = 'failed';
  } else if (UPLOAD_DONE_STATES.has(deploy)) {
    uploadStatus = 'done';
  } else if (deploy === 'queued' || deploy === 'uploading' || deploy === 'waiting') {
    uploadStatus = 'active';
  } else if (tarCaptured) {
    uploadStatus = 'active';
  } else {
    uploadStatus = 'pending';
  }

  // Stage 4 — CI running
  let ciStatus: StageStatus;
  if (FAILURE_OUTCOMES.has(deploy) && deploy !== 'upload-sha-mismatch') {
    ciStatus = 'failed';
  } else if (CI_DONE_STATES.has(deploy)) {
    ciStatus = 'done';
  } else if (CI_RUNNING_STATES.has(deploy)) {
    ciStatus = 'active';
  } else if (uploadStatus === 'done') {
    ciStatus = 'active';
  } else {
    ciStatus = 'pending';
  }

  // Stage 5 — outcome
  let outcomeStatus: StageStatus;
  if (SUCCESS_OUTCOMES.has(deploy)) {
    outcomeStatus = 'done';
  } else if (FAILURE_OUTCOMES.has(deploy)) {
    outcomeStatus = 'failed';
  } else {
    outcomeStatus = 'pending';
  }

  return [
    {
      key: 'polling',
      label: 'Polling',
      status: pollingStatus,
      detail: closed
        ? 'tab closed'
        : pollingStatus === 'done'
          ? 'tar arrived'
          : pollingStatus === 'failed'
            ? `error at status=${status || 'unknown'}`
            : `status=${status || 'pending'}`
    },
    {
      key: 'tar',
      label: 'Tar',
      status: tarStatus,
      detail: tab.archive_sha256
        ? `sha=${tab.archive_sha256.slice(0, 10)}`
        : tarStatus === 'failed'
          ? 'tar never arrived'
          : 'waiting'
    },
    {
      key: 'upload',
      label: 'Upload',
      status: uploadStatus,
      detail: deploy === 'upload-sha-mismatch'
        ? 'remote sha did not match local'
        : `deploy=${deploy || 'pending'}`
    },
    {
      key: 'ci',
      label: 'CI',
      status: ciStatus,
      detail: ciStatus === 'failed' ? `outcome=${deploy}` : `deploy=${deploy || 'pending'}`
    },
    {
      key: 'outcome',
      label: 'Outcome',
      status: outcomeStatus,
      detail: outcomeStatus === 'failed'
        ? `outcome=${deploy}`
        : outcomeStatus === 'done'
          ? 'passed'
          : 'pending'
    }
  ];
}

export function isTabClosed(tab: TabSnapshot): boolean {
  return (tab.status ?? '').toLowerCase() === 'closed';
}

export function isTabFailed(tab: TabSnapshot): boolean {
  return deriveStages(tab).some((stage) => stage.status === 'failed');
}

export function isTabPassed(tab: TabSnapshot): boolean {
  const stages = deriveStages(tab);
  return stages[stages.length - 1].status === 'done';
}

export interface OutcomeSummary {
  outcome: string;
  exitCode: string | null;
  remoteCommand: string | null;
  remoteTarget: string | null;
  logTail: string | null;
  filesChanged: string[];
  filesChangedCount: number | null;
  additions: number | null;
  deletions: number | null;
  shortstat: string | null;
  preStatus: string[];
  postStatus: string[];
  postHead: string | null;
  ciState: string | null;
  localTestsPassed: number | null;
  remoteTestsPassed: number | null;
  localSha: string | null;
  remoteSha: string | null;
}

export function summarizeOutcome(events: JailgunEvent[], tabId: number): OutcomeSummary {
  const deployFinished = events.find(
    (event) => event.kind === 'deploy-finished' && event.tab_id === tabId
  );
  const fields = deployFinished?.fields ?? {};
  const filesField = fields.changed_paths ?? fields.top_paths ?? '';
  const filesChanged = filesField
    .split(/\r?\n|,/)
    .map((value) => value.trim())
    .filter((value) => value.length > 0);
  const parseLines = (value: string | undefined) => (value ?? '')
    .split(/\r?\n/)
    .map((item) => item.trim())
    .filter((item) => item.length > 0);
  const shortstatStats = parseShortstat(fields.shortstat);
  return {
    outcome: fields.outcome ?? '',
    exitCode: fields.exit_code ?? null,
    remoteCommand: fields.remote_command ?? null,
    remoteTarget: fields.remote_target ?? null,
    logTail: fields.log_tail ?? null,
    filesChanged,
    filesChangedCount: parseOptionalNumber(fields.files_changed) ?? shortstatStats.filesChangedCount,
    additions: parseOptionalNumber(fields.additions) ?? shortstatStats.additions,
    deletions: parseOptionalNumber(fields.deletions) ?? shortstatStats.deletions,
    shortstat: fields.shortstat ?? null,
    preStatus: parseLines(fields.pre_status),
    postStatus: parseLines(fields.post_status),
    postHead: fields.post_head ?? null,
    ciState: fields.ci_state ?? null,
    localTestsPassed: parseOptionalNumber(fields.local_tests_passed) ?? parseLogTestCount(fields.log_tail),
    remoteTestsPassed: parseOptionalNumber(fields.remote_tests_passed) ?? parseLogTestCount(fields.log_tail),
    localSha: fields.local_sha256 ?? null,
    remoteSha: fields.remote_sha256 ?? null
  };
}

export type RunQualityVerdict = 'excellent' | 'healthy' | 'watching' | 'review' | 'failed' | 'pending';

export interface RunQualitySummary {
  verdict: RunQualityVerdict;
  detail: string;
  evidenceKinds: string[];
  evidenceCount: number;
  passedTabs: number;
  failedTabs: number;
  totalTabs: number;
}

export function summarizeRunQuality(
  run: RunSnapshot,
  events: JailgunEvent[],
  receipts: unknown[]
): RunQualitySummary {
  const totalTabs = run.tabs.length;
  const passedTabs = run.tabs.filter(isTabPassed).length;
  const failedTabs = run.tabs.filter(isTabFailed).length;
  const evidenceKinds = collectEvidenceKinds(events, receipts);
  const evidenceCount = events.filter(isEvidenceEvent).length + receipts.length;
  const hasErrors = failedTabs > 0 || events.some((event) => event.severity === 'error');
  const hasWarnings = events.some((event) => event.severity === 'warn');

  if (totalTabs === 0) {
    return {
      verdict: 'pending',
      detail: 'waiting for child runs',
      evidenceKinds,
      evidenceCount,
      passedTabs,
      failedTabs,
      totalTabs
    };
  }

  if (hasErrors) {
    return {
      verdict: 'failed',
      detail: `${failedTabs} failed of ${totalTabs}`,
      evidenceKinds,
      evidenceCount,
      passedTabs,
      failedTabs,
      totalTabs
    };
  }

  if (passedTabs === 0 && failedTabs === 0 && evidenceKinds.length === 0 && receipts.length === 0) {
    return {
      verdict: 'pending',
      detail: 'waiting for evidence',
      evidenceKinds,
      evidenceCount,
      passedTabs,
      failedTabs,
      totalTabs
    };
  }

  if (passedTabs === totalTabs && evidenceKinds.length > 0) {
    return {
      verdict: 'excellent',
      detail: `${passedTabs}/${totalTabs} passed · evidence ${formatEvidenceKinds(evidenceKinds)}`,
      evidenceKinds,
      evidenceCount,
      passedTabs,
      failedTabs,
      totalTabs
    };
  }

  if (passedTabs > 0 && passedTabs < totalTabs) {
    return {
      verdict: 'watching',
      detail: `${passedTabs}/${totalTabs} passing · evidence ${formatEvidenceKinds(evidenceKinds)}`,
      evidenceKinds,
      evidenceCount,
      passedTabs,
      failedTabs,
      totalTabs
    };
  }

  if (hasWarnings || evidenceKinds.length > 0 || receipts.length > 0) {
    return {
      verdict: 'review',
      detail: `${totalTabs - failedTabs}/${totalTabs} active · evidence ${formatEvidenceKinds(evidenceKinds)}`,
      evidenceKinds,
      evidenceCount,
      passedTabs,
      failedTabs,
      totalTabs
    };
  }

  return {
    verdict: 'healthy',
    detail: `${passedTabs}/${totalTabs} passed`,
    evidenceKinds,
    evidenceCount,
    passedTabs,
    failedTabs,
    totalTabs
  };
}

function parseOptionalNumber(value: string | undefined): number | null {
  if (value === undefined || value.trim() === '') return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : null;
}

function parseShortstat(value: string | undefined): Pick<OutcomeSummary, 'filesChangedCount' | 'additions' | 'deletions'> {
  const text = value ?? '';
  return {
    filesChangedCount: numberBefore(text, /\bfiles? changed\b/),
    additions: numberBefore(text, /\b(?:insertions?|additions?)\(\+\)/),
    deletions: numberBefore(text, /\bdeletions?\(-\)/)
  };
}

function numberBefore(text: string, pattern: RegExp): number | null {
  const match = text.match(new RegExp(`(\\d+)\\s+${pattern.source}`, pattern.flags));
  return match ? Number(match[1]) : null;
}

function parseLogTestCount(value: string | undefined): number | null {
  const text = value ?? '';
  const matches = [...text.matchAll(/(?:cargo test|npm test|vitest|tests?)\s*:\s*(\d+)\s+passed/gi)];
  if (matches.length === 0) return null;
  return matches.reduce((sum, match) => sum + Number(match[1]), 0);
}

function collectEvidenceKinds(events: JailgunEvent[], receipts: unknown[]): string[] {
  const kinds = new Set<string>();
  for (const event of events) {
    if (isEvidenceEvent(event)) {
      kinds.add(event.kind);
    }
  }
  if (receipts.length > 0) {
    kinds.add('receipt');
  }
  return Array.from(kinds);
}

function isEvidenceEvent(event: JailgunEvent): boolean {
  return (
    event.kind === 'download-receipt' ||
    event.kind === 'deploy-finished' ||
    event.kind === 'remote-safety' ||
    event.kind === 'rate-limit-detected' ||
    event.kind === 'error'
  );
}

function formatEvidenceKinds(kinds: string[]): string {
  if (kinds.length === 0) {
    return 'none';
  }
  return kinds.slice(0, 3).join(', ');
}

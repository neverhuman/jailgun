import type { JailgunEvent, TabSnapshot } from '../types';

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
  shortstat: string | null;
  preStatus: string[];
  postStatus: string[];
  postHead: string | null;
  ciState: string | null;
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
  return {
    outcome: fields.outcome ?? '',
    exitCode: fields.exit_code ?? null,
    remoteCommand: fields.remote_command ?? null,
    remoteTarget: fields.remote_target ?? null,
    logTail: fields.log_tail ?? null,
    filesChanged,
    shortstat: fields.shortstat ?? null,
    preStatus: parseLines(fields.pre_status),
    postStatus: parseLines(fields.post_status),
    postHead: fields.post_head ?? null,
    ciState: fields.ci_state ?? null,
    localSha: fields.local_sha256 ?? null,
    remoteSha: fields.remote_sha256 ?? null
  };
}

import { fixtureEvents, fixtureReceipts, fixtureRuns } from './fixtures';
import type { JailgunEvent, ReceiptResponse, RunSnapshot, TabSnapshot } from './types';

export type DashboardDataMode = 'api' | 'fixture';

export interface DashboardRequestOptions {
  mode?: DashboardDataMode;
  fetcher?: typeof fetch;
}

export interface EventSubscriptionOptions {
  mode?: DashboardDataMode;
  onError?: (error: Error) => void;
}

export async function fetchRuns(options: DashboardRequestOptions = {}): Promise<RunSnapshot[]> {
  const mode = options.mode ?? 'api';
  if (mode === 'fixture') {
    return fixtureRuns;
  }
  const fetcher = options.fetcher ?? fetch;
  const response = await fetcher('/api/runs');
  if (!response.ok) {
    throw new Error(`GET /api/runs failed ${response.status}`);
  }
  return decodeRunSnapshots(await response.json());
}

export async function fetchReceipts(
  runId: string,
  options: DashboardRequestOptions = {}
): Promise<ReceiptResponse> {
  const mode = options.mode ?? 'api';
  if (mode === 'fixture') {
    return { ...fixtureReceipts, run_id: runId };
  }
  const fetcher = options.fetcher ?? fetch;
  const response = await fetcher(`/api/receipts/${encodeURIComponent(runId)}`);
  if (!response.ok) {
    throw new Error(`GET /api/receipts failed ${response.status}`);
  }
  return decodeReceiptResponse(await response.json());
}

export function subscribeEvents(
  onEvent: (event: JailgunEvent) => void,
  options: EventSubscriptionOptions = {}
): () => void {
  const mode = options.mode ?? 'api';
  if (mode === 'fixture') {
    fixtureEvents.forEach(onEvent);
    return () => undefined;
  }
  if (typeof WebSocket === 'undefined') {
    throw new Error('WebSocket is unavailable in API mode');
  }
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const url = `${protocol}//${window.location.host}/ws/events`;
  const socket = new WebSocket(url);
  const openTimer = window.setTimeout(() => {
    if (socket.readyState !== WebSocket.OPEN) {
      options.onError?.(new Error('event stream did not open before timeout'));
    }
  }, 250);
  socket.onmessage = (message) => {
    try {
      onEvent(decodeJailgunEvent(JSON.parse(message.data)));
    } catch (error) {
      options.onError?.(error instanceof Error ? error : new Error(String(error)));
    }
  };
  socket.onerror = () => {
    options.onError?.(new Error('event stream error'));
  };
  return () => {
    window.clearTimeout(openTimer);
    socket.close();
  };
}

function decodeRunSnapshots(value: unknown): RunSnapshot[] {
  if (!Array.isArray(value)) {
    throw new Error('invalid run snapshot payload: expected array');
  }
  return value.map(decodeRunSnapshot);
}

function decodeRunSnapshot(value: unknown): RunSnapshot {
  const record = expectRecord(value, 'run snapshot');
  const tabs = expectArray(record.tabs, 'run snapshot tabs').map(decodeTabSnapshot);
  return {
    run_id: expectString(record.run_id, 'run_id'),
    started_at: expectString(record.started_at, 'started_at'),
    finished_at: expectNullableString(record.finished_at, 'finished_at'),
    status: expectString(record.status, 'status'),
    tabs,
    deploy_queue: expectDeployQueue(record.deploy_queue),
    denied_github_prompts: expectNumber(record.denied_github_prompts, 'denied_github_prompts'),
    allowed_info_prompts: expectNumber(record.allowed_info_prompts, 'allowed_info_prompts')
  };
}

function decodeTabSnapshot(value: unknown): TabSnapshot {
  const record = expectRecord(value, 'tab snapshot');
  return {
    tab_id: expectNumber(record.tab_id, 'tab_id'),
    status: expectString(record.status, 'tab status'),
    page_url: expectString(record.page_url, 'page_url'),
    archive_sha256: expectNullableString(record.archive_sha256, 'archive_sha256'),
    download_latency_ms: expectNullableNumber(record.download_latency_ms, 'download_latency_ms'),
    deploy_status: expectString(record.deploy_status, 'deploy_status'),
    prompt_policy_decision: expectNullableString(record.prompt_policy_decision, 'prompt_policy_decision')
  };
}

function decodeReceiptResponse(value: unknown): ReceiptResponse {
  const record = expectRecord(value, 'receipt response');
  return {
    run_id: expectString(record.run_id, 'run_id'),
    receipts: expectArray(record.receipts, 'receipts')
  };
}

function decodeJailgunEvent(value: unknown): JailgunEvent {
  const record = expectRecord(value, 'event');
  return {
    run_id: expectString(record.run_id, 'event run_id'),
    tab_id: expectNullableNumber(record.tab_id, 'event tab_id'),
    timestamp: expectString(record.timestamp, 'event timestamp'),
    kind: expectString(record.kind, 'event kind'),
    severity: expectSeverity(record.severity),
    message: expectString(record.message, 'event message'),
    fields: expectStringRecord(record.fields, 'event fields')
  };
}

function expectRecord(value: unknown, label: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(`invalid ${label}: expected object`);
  }
  return Object.fromEntries(Object.entries(value));
}

function expectArray(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`invalid ${label}: expected array`);
  }
  return value;
}

function expectString(value: unknown, label: string): string {
  if (typeof value !== 'string') {
    throw new Error(`invalid ${label}: expected string`);
  }
  return value;
}

function expectNullableString(value: unknown, label: string): string | null {
  return value === null ? value : expectString(value, label);
}

function expectNumber(value: unknown, label: string): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    throw new Error(`invalid ${label}: expected finite number`);
  }
  return value;
}

function expectNullableNumber(value: unknown, label: string): number | null {
  return value === null ? value : expectNumber(value, label);
}

function expectDeployQueue(value: unknown): RunSnapshot['deploy_queue'] {
  if (
    value === 'idle' ||
    value === 'waiting' ||
    value === 'running' ||
    value === 'blocked' ||
    value === 'done'
  ) {
    return value;
  }
  throw new Error('invalid deploy_queue value');
}

function expectSeverity(value: unknown): JailgunEvent['severity'] {
  if (value === 'debug' || value === 'info' || value === 'warn' || value === 'error') {
    return value;
  }
  throw new Error('invalid event severity value');
}

function expectStringRecord(value: unknown, label: string): Record<string, string> {
  const record = expectRecord(value, label);
  return Object.fromEntries(
    Object.entries(record).map(([key, item]) => [key, expectString(item, `${label}.${key}`)])
  );
}

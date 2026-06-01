import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { fetchReceipts, fetchRuns, subscribeEvents } from './api';
import type { JailgunEvent, RunSnapshot } from './types';

export type DataSource = 'api' | 'fixture';
export type EventStreamStatus = 'connecting' | 'open' | 'closed';

export interface DashboardState {
  runs: RunSnapshot[];
  selectedRunId: string | null;
  selectedRun: RunSnapshot | null;
  receipts: unknown[];
  events: JailgunEvent[];
  connection: EventStreamStatus;
  dataSource: DataSource;
  error: string | null;
  lastEventAt: Record<number, number>;
  selectRun: (runId: string) => void;
  refresh: () => Promise<void>;
}

export function useDashboardData(): DashboardState {
  const [runs, setRuns] = useState<RunSnapshot[]>([]);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [receipts, setReceipts] = useState<unknown[]>([]);
  const [events, setEvents] = useState<JailgunEvent[]>([]);
  const [connection, setConnection] = useState<EventStreamStatus>('connecting');
  const [dataSource, setDataSource] = useState<DataSource>('api');
  const [error, setError] = useState<string | null>(null);
  const [lastEventAt, setLastEventAt] = useState<Record<number, number>>({});
  const selectedRunIdRef = useRef<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const nextRuns = await fetchRuns();
      setRuns(nextRuns);
      setDataSource('api');
      setError(null);
      setSelectedRunId((current) => {
        const next = current ?? nextRuns[0]?.run_id ?? null;
        selectedRunIdRef.current = next;
        return next;
      });
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : String(loadError));
      setDataSource('fixture');
      const fixtureRuns = await fetchRuns({ mode: 'fixture' });
      setRuns(fixtureRuns);
      setSelectedRunId((current) => {
        const next = current ?? fixtureRuns[0]?.run_id ?? null;
        selectedRunIdRef.current = next;
        return next;
      });
    }
  }, []);

  const selectRun = useCallback((runId: string) => {
    selectedRunIdRef.current = runId;
    setSelectedRunId(runId);
  }, []);

  useEffect(() => {
    selectedRunIdRef.current = selectedRunId;
  }, [dataSource, selectedRunId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    let unsubscribe: () => void = () => undefined;
    try {
      unsubscribe = subscribeEvents(
        (event) => {
          setConnection('open');
          setEvents((current) => [event, ...current].slice(0, 80));
          setRuns((current) => applyEventToRuns(current, event));
          setSelectedRunId((current) => current ?? event.run_id);
          if (event.tab_id !== null) {
            const tabId = event.tab_id;
            setLastEventAt((current) => ({ ...current, [tabId]: Date.now() }));
          }
        },
        { mode: dataSource, onError: (eventError) => setError(eventError.message) }
      );
    } catch (eventError) {
      setError(eventError instanceof Error ? eventError.message : String(eventError));
    }
    return () => {
      setConnection('closed');
      unsubscribe();
    };
  }, [dataSource]);

  useEffect(() => {
    if (!selectedRunId) {
      setReceipts([]);
      return;
    }
    let ignore = false;
    void fetchReceipts(selectedRunId, { mode: dataSource })
      .then((result) => {
        if (!ignore) {
          setReceipts(result.receipts);
        }
      })
      .catch(() => {
        if (!ignore) {
          setReceipts([]);
        }
      });
    return () => {
      ignore = true;
    };
  }, [dataSource, selectedRunId]);

  const selectedRun = useMemo(
    () => runs.find((run) => run.run_id === selectedRunId) ?? null,
    [runs, selectedRunId]
  );

  return {
    runs,
    selectedRunId,
    selectedRun,
    receipts,
    events,
    connection,
    dataSource,
    error,
    lastEventAt,
    selectRun,
    refresh
  };
}

function applyEventToRuns(runs: RunSnapshot[], event: JailgunEvent): RunSnapshot[] {
  const index = runs.findIndex((run) => run.run_id === event.run_id);
  if (index === -1) {
    return [createRunFromEvent(event), ...runs];
  }
  return runs.map((run) => (run.run_id === event.run_id ? applyEventToRun(run, event) : run));
}

function createRunFromEvent(event: JailgunEvent): RunSnapshot {
  const metadata = runLoopMetadataFromEvent(event);
  return {
    run_id: event.run_id,
    started_at: event.timestamp,
    finished_at: null,
    status: event.fields.status ?? 'running',
    batch_tabs: metadata.batch_tabs,
    loop_count: metadata.loop_count,
    planned_tabs: metadata.planned_tabs,
    deploy_queue: event.kind === 'deploy-queued' ? 'waiting' : 'idle',
    denied_github_prompts: event.fields.decision === 'deny' ? 1 : 0,
    allowed_info_prompts: event.fields.decision === 'allow-info' ? 1 : 0,
    tabs: event.tab_id === null ? [] : [applyEventToTab(defaultTabSnapshot(event.tab_id), event)],
    early_stops_succeeded: 0,
    early_stops_attempted: 0
  };
}

function applyEventToRun(run: RunSnapshot, event: JailgunEvent): RunSnapshot {
  const decision = event.fields.decision;
  const metadata = event.kind === 'run-started' ? runLoopMetadataFromEvent(event, run) : run;
  const tabs = event.tab_id === null ? run.tabs : upsertTab(run.tabs, event);
  const earlyStopTotals = recomputeEarlyStopTotals(tabs);
  return {
    ...run,
    ...metadata,
    status: event.fields.status ?? run.status,
    finished_at: event.kind === 'deploy-finished' ? event.timestamp : run.finished_at,
    deploy_queue: queueStateForEvent(event, run.deploy_queue),
    denied_github_prompts: decision === 'deny' ? run.denied_github_prompts + 1 : run.denied_github_prompts,
    allowed_info_prompts: decision === 'allow-info' ? run.allowed_info_prompts + 1 : run.allowed_info_prompts,
    tabs,
    early_stops_succeeded: earlyStopTotals.succeeded,
    early_stops_attempted: earlyStopTotals.attempted
  };
}

function recomputeEarlyStopTotals(tabs: RunSnapshot['tabs']): { succeeded: number; attempted: number } {
  let succeeded = 0;
  let attempted = 0;
  for (const tab of tabs) {
    if (tab.early_stop_outcome === 'succeeded') {
      succeeded += 1;
      attempted += 1;
    } else if (tab.early_stop_outcome === 'attempted') {
      attempted += 1;
    }
  }
  return { succeeded, attempted };
}

function defaultTabSnapshot(tabId: number): RunSnapshot['tabs'][number] {
  return {
    tab_id: tabId,
    status: 'active',
    page_url: '',
    archive_sha256: null,
    download_latency_ms: null,
    deploy_status: 'pending',
    prompt_policy_decision: null,
    early_stop_outcome: null
  };
}

function runLoopMetadataFromEvent(
  event: JailgunEvent,
  existing?: Pick<RunSnapshot, 'batch_tabs' | 'loop_count' | 'planned_tabs'>
): Pick<RunSnapshot, 'batch_tabs' | 'loop_count' | 'planned_tabs'> {
  const batchTabs =
    parseOptionalNumber(event.fields.batch_tabs) ??
    parseOptionalNumber(event.fields.tabs) ??
    existing?.batch_tabs ??
    0;
  const loopCount = parseOptionalNumber(event.fields.loop_count) ?? existing?.loop_count ?? 0;
  const computedPlannedTabs = batchTabs > 0 ? batchTabs * (loopCount + 1) : null;
  const plannedTabs =
    parseOptionalNumber(event.fields.planned_tabs) ??
    computedPlannedTabs ??
    existing?.planned_tabs ??
    0;
  return {
    batch_tabs: batchTabs,
    loop_count: loopCount,
    planned_tabs: plannedTabs
  };
}

function upsertTab(tabs: RunSnapshot['tabs'], event: JailgunEvent): RunSnapshot['tabs'] {
  if (event.tab_id === null) return tabs;
  const existing = tabs.find((tab) => tab.tab_id === event.tab_id);
  const next = applyEventToTab(existing ?? defaultTabSnapshot(event.tab_id), event);
  if (!existing) {
    return [...tabs, next].sort((left, right) => left.tab_id - right.tab_id);
  }
  return tabs.map((tab) => (tab.tab_id === event.tab_id ? next : tab));
}

function applyEventToTab(tab: RunSnapshot['tabs'][number], event: JailgunEvent): RunSnapshot['tabs'][number] {
  return {
    ...tab,
    status: tabStatusForEvent(event, tab.status),
    page_url: event.fields.page_url ?? tab.page_url,
    archive_sha256: event.fields.sha256 ?? tab.archive_sha256,
    download_latency_ms: parseOptionalNumber(event.fields.download_latency_ms) ?? tab.download_latency_ms,
    deploy_status: deployStatusForEvent(event, tab.deploy_status),
    prompt_policy_decision: event.fields.decision ?? tab.prompt_policy_decision,
    early_stop_outcome: nextEarlyStopOutcome(tab.early_stop_outcome, event)
  };
}

function nextEarlyStopOutcome(
  current: RunSnapshot['tabs'][number]['early_stop_outcome'],
  event: JailgunEvent
): RunSnapshot['tabs'][number]['early_stop_outcome'] {
  if (event.kind !== 'generation-stopped') return current;
  const phase = event.fields.phase ?? '';
  if (phase !== 'pre-download' && phase !== 'post-download') return current;
  const method = event.fields.method ?? '';
  const isSuccess =
    method.length > 0 &&
    !method.startsWith('not-active') &&
    !method.startsWith('not-run') &&
    !method.startsWith('shutdown');
  if (isSuccess) return 'succeeded';
  if (current === 'succeeded') return 'succeeded';
  return 'attempted';
}

function tabStatusForEvent(event: JailgunEvent, current: string): string {
  if (event.fields.tab_status) return event.fields.tab_status;
  if (event.kind === 'download-started') return 'downloading';
  if (event.kind === 'download-receipt') return 'downloaded';
  if (event.kind === 'generation-stopped') return 'generation-stopped';
  if (event.kind === 'deploy-finished') return event.severity === 'error' ? 'error' : 'deployed';
  if (event.kind === 'tab-closed') return 'closed';
  return current;
}

function deployStatusForEvent(event: JailgunEvent, current: string): string {
  if (event.fields.deploy_status) return event.fields.deploy_status;
  if (event.kind === 'deploy-queued') return event.fields.status ?? 'queued';
  if (event.kind === 'remote-safety') return event.fields.phase ?? current;
  if (event.kind === 'deploy-finished') return event.fields.outcome ?? 'done';
  return current;
}

function queueStateForEvent(event: JailgunEvent, current: RunSnapshot['deploy_queue']): RunSnapshot['deploy_queue'] {
  if (event.kind === 'deploy-queued') return 'waiting';
  if (event.kind === 'remote-safety') return event.fields.outcome === 'blocked' ? 'blocked' : 'running';
  if (event.kind === 'deploy-finished') return 'done';
  return current;
}

function parseOptionalNumber(value: string | undefined): number | null {
  const parsed = value === undefined || value === '' ? undefined : Number(value);
  return parsed !== undefined && Number.isFinite(parsed) ? parsed : null;
}

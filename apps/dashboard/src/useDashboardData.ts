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
    void fetchReceipts(selectedRunId, { mode: dataSource }).then((result) => {
      if (!ignore) {
        setReceipts(result.receipts);
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
  return {
    run_id: event.run_id,
    started_at: event.timestamp,
    finished_at: null,
    status: event.fields.status ?? 'running',
    deploy_queue: event.kind === 'deploy-queued' ? 'waiting' : 'idle',
    denied_github_prompts: event.fields.decision === 'deny' ? 1 : 0,
    allowed_info_prompts: event.fields.decision === 'allow-info' ? 1 : 0,
    tabs: event.tab_id === null ? [] : [{
      tab_id: event.tab_id,
      status: tabStatusForEvent(event, event.fields.tab_status ?? 'active'),
      page_url: event.fields.page_url ?? '',
      archive_sha256: event.fields.sha256 ?? null,
      download_latency_ms: parseOptionalNumber(event.fields.download_latency_ms),
      deploy_status: deployStatusForEvent(event, event.fields.deploy_status ?? 'pending'),
      prompt_policy_decision: event.fields.decision ?? null
    }]
  };
}

function applyEventToRun(run: RunSnapshot, event: JailgunEvent): RunSnapshot {
  const decision = event.fields.decision;
  const tabs = event.tab_id === null ? run.tabs : upsertTab(run.tabs, event);
  return {
    ...run,
    status: event.fields.status ?? run.status,
    finished_at: event.kind === 'deploy-finished' ? event.timestamp : run.finished_at,
    deploy_queue: queueStateForEvent(event, run.deploy_queue),
    denied_github_prompts: decision === 'deny' ? run.denied_github_prompts + 1 : run.denied_github_prompts,
    allowed_info_prompts: decision === 'allow-info' ? run.allowed_info_prompts + 1 : run.allowed_info_prompts,
    tabs
  };
}

function upsertTab(tabs: RunSnapshot['tabs'], event: JailgunEvent): RunSnapshot['tabs'] {
  if (event.tab_id === null) return tabs;
  const existing = tabs.find((tab) => tab.tab_id === event.tab_id);
  const next = applyEventToTab(
    existing ?? {
      tab_id: event.tab_id,
      status: 'active',
      page_url: '',
      archive_sha256: null,
      download_latency_ms: null,
      deploy_status: 'pending',
      prompt_policy_decision: null
    },
    event
  );
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
    prompt_policy_decision: event.fields.decision ?? tab.prompt_policy_decision
  };
}

function tabStatusForEvent(event: JailgunEvent, current: string): string {
  if (event.fields.tab_status) return event.fields.tab_status;
  if (event.kind === 'download-receipt') return 'downloaded';
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

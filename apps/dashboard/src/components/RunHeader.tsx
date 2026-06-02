import { Activity, AlertTriangle, CheckCircle2, Download, GitBranch, Hand, Lock, Send, XCircle } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';

import type { JailgunEvent, RunSnapshot } from '../types';
import { isTabClosed, isTabFailed, isTabPassed, summarizeOutcome, summarizeRunQuality } from './stages';

interface RunHeaderProps {
  run: RunSnapshot;
  connection: string;
  dataSource: string;
  events?: JailgunEvent[];
  receipts?: unknown[];
}

export function RunHeader({ run, connection, dataSource, events = [], receipts = [] }: RunHeaderProps) {
  const tabs = run.tabs;
  const loopsRemaining = useMemo(
    () => calculateLoopsRemaining(run.loop_count, run.batch_tabs, tabs.length),
    [run.batch_tabs, run.loop_count, tabs.length]
  );
  const quality = useMemo(() => summarizeRunQuality(run, events, receipts), [events, receipts, run]);
  const downloaded = tabs.filter((tab) => tab.archive_sha256).length;
  const passed = tabs.filter(isTabPassed).length;
  const failed = tabs.filter(isTabFailed).length;
  const closed = tabs.filter(isTabClosed).length;
  const inFlight = tabs.length - passed - failed;
  const tabsValue = run.loop_count > 0 ? `${tabs.length}/${run.planned_tabs}` : tabs.length;
  const pushed = useMemo(
    () => tabs.filter((tab) => Boolean(summarizeOutcome(events, tab.tab_id).postHead)).length,
    [events, tabs]
  );

  return (
    <header className="runHeader" aria-label="run header">
      <div className="runHeaderTop">
        <div>
          <h1>Jailgun</h1>
          <p>
            {run.run_id} · {run.status} · {dataSource} · {connection}
          </p>
        </div>
        <RunElapsed startedAt={run.started_at} finishedAt={run.finished_at} />
      </div>
      {run.loop_count > 0 ? (
        <div className="runLoopBanner" aria-label="looping status">
          <span className="runLoopBannerLabel">Looping</span>
          <strong className="runLoopBannerValue">{loopsRemaining} left</strong>
          <span className="runLoopBannerMeta">
            {run.batch_tabs} per batch · {run.planned_tabs} planned
          </span>
        </div>
      ) : null}
      <div className="runHeaderMetrics" aria-label="run progress metrics">
        <RunMetric icon={<Activity size={18} />} label="Tabs" value={tabsValue} />
        <RunMetric icon={<Download size={18} />} label="Tar captured" value={downloaded} tone="ok" />
        <RunMetric icon={<CheckCircle2 size={18} />} label="Passed" value={passed} tone="ok" />
        <RunMetric icon={<XCircle size={18} />} label="Failed" value={failed} tone={failed > 0 ? 'danger' : 'neutral'} />
        <RunMetric icon={<Send size={18} />} label="Pushed" value={pushed} tone={pushed > 0 ? 'ok' : 'neutral'} />
        <RunMetric icon={<Lock size={18} />} label="Closed" value={closed} />
        <RunMetric icon={<GitBranch size={18} />} label="Deploy queue" value={run.deploy_queue} />
        <RunMetric icon={<AlertTriangle size={18} />} label="In flight" value={inFlight} tone={inFlight > 0 ? 'warn' : 'neutral'} />
        <RunMetric
          icon={qualityIcon(quality.verdict)}
          label="Quality"
          value={quality.verdict}
          detail={quality.detail}
          tone={qualityTone(quality.verdict)}
        />
        <RunMetric
          icon={<Hand size={18} />}
          label="Early stops"
          value={`${run.early_stops_succeeded}/${run.early_stops_attempted}`}
          tone={earlyStopTone(run.early_stops_succeeded, run.early_stops_attempted)}
        />
      </div>
    </header>
  );
}

function earlyStopTone(
  succeeded: number,
  attempted: number
): 'neutral' | 'ok' | 'warn' | 'danger' {
  if (attempted === 0) return 'neutral';
  if (succeeded === attempted) return 'ok';
  if (succeeded === 0) return 'danger';
  return 'warn';
}

function calculateLoopsRemaining(loopCount: number, batchTabs: number, observedTabs: number): number {
  if (loopCount <= 0 || batchTabs <= 0) {
    return 0;
  }
  const batchesStarted = Math.max(0, Math.ceil(observedTabs / batchTabs) - 1);
  return Math.max(0, loopCount - batchesStarted);
}

function RunMetric({
  icon,
  label,
  value,
  detail,
  tone = 'neutral'
}: {
  icon: React.ReactNode;
  label: string;
  value: string | number;
  detail?: string;
  tone?: 'neutral' | 'ok' | 'warn' | 'danger';
}) {
  return (
    <div className={`runMetric ${tone}`}>
      {icon}
      <span className="runMetricLabel">{label}</span>
      <strong className="runMetricValue">{value}</strong>
      {detail ? <span className="runMetricDetail">{detail}</span> : null}
    </div>
  );
}

function RunElapsed({ startedAt, finishedAt }: { startedAt: string; finishedAt: string | null }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (finishedAt) return undefined;
    const interval = window.setInterval(() => setNow(Date.now()), 1_000);
    return () => window.clearInterval(interval);
  }, [finishedAt]);
  const started = Date.parse(startedAt);
  const ended = finishedAt ? Date.parse(finishedAt) : now;
  const elapsedMs = Number.isFinite(started) && Number.isFinite(ended) ? Math.max(0, ended - started) : 0;
  return (
    <div className="runElapsed" aria-label="elapsed time">
      <span className="runElapsedLabel">{finishedAt ? 'Total' : 'Elapsed'}</span>
      <strong className="runElapsedValue">{formatElapsed(elapsedMs)}</strong>
    </div>
  );
}

function formatElapsed(ms: number): string {
  const totalSeconds = Math.floor(ms / 1_000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  if (minutes >= 60) {
    const hours = Math.floor(minutes / 60);
    const remainingMinutes = minutes % 60;
    return `${hours}h ${remainingMinutes}m ${seconds}s`;
  }
  return `${minutes}m ${seconds.toString().padStart(2, '0')}s`;
}

function qualityTone(
  verdict: 'excellent' | 'healthy' | 'watching' | 'review' | 'failed' | 'pending'
): 'neutral' | 'ok' | 'warn' | 'danger' {
  if (verdict === 'excellent' || verdict === 'healthy') return 'ok';
  if (verdict === 'watching' || verdict === 'review') return 'warn';
  if (verdict === 'failed') return 'danger';
  return 'neutral';
}

function qualityIcon(verdict: 'excellent' | 'healthy' | 'watching' | 'review' | 'failed' | 'pending') {
  if (verdict === 'failed') return <XCircle size={18} />;
  if (verdict === 'watching' || verdict === 'review') return <AlertTriangle size={18} />;
  if (verdict === 'excellent' || verdict === 'healthy') return <CheckCircle2 size={18} />;
  return <Activity size={18} />;
}

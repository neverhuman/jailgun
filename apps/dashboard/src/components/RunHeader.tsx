import { Activity, AlertTriangle, CheckCircle2, Download, GitBranch, Lock, Send, XCircle } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';

import type { JailgunEvent, RunSnapshot } from '../types';
import { isTabClosed, isTabFailed, isTabPassed, summarizeOutcome } from './stages';

interface RunHeaderProps {
  run: RunSnapshot;
  connection: string;
  dataSource: string;
  events?: JailgunEvent[];
}

export function RunHeader({ run, connection, dataSource, events = [] }: RunHeaderProps) {
  const tabs = run.tabs;
  const downloaded = tabs.filter((tab) => tab.archive_sha256).length;
  const passed = tabs.filter(isTabPassed).length;
  const failed = tabs.filter(isTabFailed).length;
  const closed = tabs.filter(isTabClosed).length;
  const inFlight = tabs.length - passed - failed;
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
      <div className="runHeaderMetrics" aria-label="run progress metrics">
        <RunMetric icon={<Activity size={18} />} label="Tabs" value={tabs.length} />
        <RunMetric icon={<Download size={18} />} label="Tar captured" value={downloaded} tone="ok" />
        <RunMetric icon={<CheckCircle2 size={18} />} label="Passed" value={passed} tone="ok" />
        <RunMetric icon={<XCircle size={18} />} label="Failed" value={failed} tone={failed > 0 ? 'danger' : 'neutral'} />
        <RunMetric icon={<Send size={18} />} label="Pushed" value={pushed} tone={pushed > 0 ? 'ok' : 'neutral'} />
        <RunMetric icon={<Lock size={18} />} label="Closed" value={closed} />
        <RunMetric icon={<GitBranch size={18} />} label="Deploy queue" value={run.deploy_queue} />
        <RunMetric icon={<AlertTriangle size={18} />} label="In flight" value={inFlight} tone={inFlight > 0 ? 'warn' : 'neutral'} />
      </div>
    </header>
  );
}

function RunMetric({
  icon,
  label,
  value,
  tone = 'neutral'
}: {
  icon: React.ReactNode;
  label: string;
  value: string | number;
  tone?: 'neutral' | 'ok' | 'warn' | 'danger';
}) {
  return (
    <div className={`runMetric ${tone}`}>
      {icon}
      <span className="runMetricLabel">{label}</span>
      <strong className="runMetricValue">{value}</strong>
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

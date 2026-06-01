import { Activity, AlertTriangle, CheckCircle2, Download, GitBranch, Shield, XCircle } from 'lucide-react';
import { useMemo } from 'react';

import type { JailgunEvent, RunSnapshot, TabSnapshot } from './types';
import { useDashboardData } from './useDashboardData';

export function App() {
  const { runs, selectedRun: activeRun, receipts, events, connection, dataSource, error } = useDashboardData();
  const metrics = useMemo(() => computeMetrics(activeRun), [activeRun]);

  return (
    <main className="shell">
      <header className="topbar">
        <div>
          <h1>Jailgun</h1>
          <p>{activeRun ? `${activeRun.run_id} · ${activeRun.status} · ${dataSource} · ${connection}` : `run monitor · ${dataSource} · ${connection}`}</p>
        </div>
        <div className="policyCounters" aria-label="GitHub prompt counters">
          <Metric icon={<Shield size={18} />} label="Denied" value={activeRun?.denied_github_prompts ?? 0} tone="danger" />
          <Metric icon={<CheckCircle2 size={18} />} label="Info Allowed" value={activeRun?.allowed_info_prompts ?? 0} tone="ok" />
        </div>
      </header>

      {error ? <div className="notice errorState">{error}</div> : null}

      {!activeRun ? (
        <section className="emptyState" aria-label="empty state">
          <Activity size={28} />
          <h2>No runs yet</h2>
          <p>Waiting for run snapshots.</p>
        </section>
      ) : (
        <>
          <section className="metricGrid" aria-label="run metrics">
            <Metric icon={<Activity size={18} />} label="Progress" value={`${metrics.completed}/${metrics.total}`} />
            <Metric icon={<Download size={18} />} label="Median Download" value={`${metrics.medianLatencyMs}ms`} />
            <Metric icon={<GitBranch size={18} />} label="Deploy Queue" value={activeRun.deploy_queue} />
            <Metric icon={<AlertTriangle size={18} />} label="Remote Safety" value={remoteSafety(events)} tone="warn" />
          </section>

          <section className="layout">
            <div className="panel wide">
              <div className="panelHeader">
                <h2>Live Runs</h2>
                <span>{runs.length} run</span>
              </div>
              <RunsTable runs={runs} />
            </div>
            <div className="panel">
              <div className="panelHeader">
                <h2>Progress</h2>
                <span>{metrics.deployDone} deployed</span>
              </div>
              <ProgressChart tabs={activeRun.tabs} />
            </div>
          </section>

          <section className="tabGrid" aria-label="tab status cards">
            {activeRun.tabs.map((tab) => (
              <TabCard key={tab.tab_id} tab={tab} />
            ))}
          </section>

          <section className="layout">
            <div className="panel">
              <div className="panelHeader">
                <h2>Receipt Timeline</h2>
                <span>{receipts.length} receipts</span>
              </div>
              <ReceiptTimeline receipts={receipts} events={events} />
            </div>
            <div className="panel">
              <div className="panelHeader">
                <h2>Event Stream</h2>
                <span>{events.length} events</span>
              </div>
              <EventList events={events} />
            </div>
          </section>
        </>
      )}
    </main>
  );
}

function RunsTable({ runs }: { runs: RunSnapshot[] }) {
  return (
    <table>
      <thead>
        <tr>
          <th>Run</th>
          <th>Status</th>
          <th>Tabs</th>
          <th>Queue</th>
          <th>GitHub Prompts</th>
        </tr>
      </thead>
      <tbody>
        {runs.map((run) => (
          <tr key={run.run_id}>
            <td>{run.run_id}</td>
            <td>{run.status}</td>
            <td>{run.tabs.length}</td>
            <td>{run.deploy_queue}</td>
            <td>{run.denied_github_prompts} denied / {run.allowed_info_prompts} info</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function TabCard({ tab }: { tab: TabSnapshot }) {
  const ok = tab.deploy_status === 'done' || tab.deploy_status === 'validated';
  return (
    <article className="tabCard">
      <div className="tabCardHeader">
        <h3>Tab {tab.tab_id}</h3>
        {ok ? <CheckCircle2 size={18} /> : <XCircle size={18} />}
      </div>
      <dl>
        <dt>Status</dt>
        <dd>{tab.status}</dd>
        <dt>Deploy</dt>
        <dd>{tab.deploy_status}</dd>
        <dt>Archive</dt>
        <dd>{tab.archive_sha256 ? tab.archive_sha256.slice(0, 10) : 'pending'}</dd>
        <dt>Policy</dt>
        <dd>{tab.prompt_policy_decision ?? 'none'}</dd>
      </dl>
    </article>
  );
}

function ProgressChart({ tabs }: { tabs: TabSnapshot[] }) {
  const maxLatency = Math.max(...tabs.map((tab) => tab.download_latency_ms ?? 0), 1);
  return (
    <div className="chart" aria-label="download latency chart">
      {tabs.map((tab) => {
        const height = Math.max(10, Math.round(((tab.download_latency_ms ?? 0) / maxLatency) * 100));
        return (
          <div className="barWrap" key={tab.tab_id}>
            <div className="bar" style={{ height: `${height}%` }} />
            <span>{tab.tab_id}</span>
          </div>
        );
      })}
    </div>
  );
}

function ReceiptTimeline({ receipts, events }: { receipts: unknown[]; events: JailgunEvent[] }) {
  const receiptItems = receipts;
  const downloadEvents = events.filter((event) => event.kind.includes('receipt') || event.kind.includes('download'));
  const items = receiptItems.length > 0 ? receiptItems : downloadEvents;
  if (items.length === 0) {
    return <p className="muted">No receipts yet.</p>;
  }
  return (
    <ol className="timeline">
      {items.map((item, index) => (
        <li key={index}>
          <span className="dot" />
          <code>{formatReceipt(item)}</code>
        </li>
      ))}
    </ol>
  );
}

function EventList({ events }: { events: JailgunEvent[] }) {
  if (events.length === 0) {
    return <p className="muted">Waiting for WebSocket events.</p>;
  }
  return (
    <ul className="events">
      {events.map((event, index) => (
        <li key={`${event.timestamp}-${index}`}>
          <span className={`severity ${event.severity}`}>{event.severity}</span>
          <span>{event.kind}</span>
          <strong>{event.message}</strong>
        </li>
      ))}
    </ul>
  );
}

function Metric({ icon, label, value, tone = 'neutral' }: { icon: React.ReactNode; label: string; value: string | number; tone?: 'neutral' | 'ok' | 'warn' | 'danger' }) {
  return (
    <div className={`metric ${tone}`}>
      {icon}
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function computeMetrics(run: RunSnapshot | null) {
  const tabs = run?.tabs ?? [];
  const latencies = tabs.map((tab) => tab.download_latency_ms).filter((value): value is number => value !== null).sort((a, b) => a - b);
  const medianLatencyMs = latencies.length ? latencies[Math.floor(latencies.length / 2)] : 0;
  return {
    total: tabs.length,
    completed: tabs.filter((tab) => tab.archive_sha256).length,
    deployDone: tabs.filter((tab) => tab.deploy_status === 'done' || tab.deploy_status === 'validated').length,
    medianLatencyMs
  };
}

function remoteSafety(events: JailgunEvent[]) {
  return events.find((event) => event.kind === 'remote-safety')?.fields.policy ?? 'preserve-reset';
}

function formatReceipt(item: unknown): string {
  if (typeof item !== 'object' || item === null) {
    return String(item);
  }
  const record = Object.fromEntries(Object.entries(item));
  const tab = record.tab_id ? `tab ${record.tab_id}` : 'run';
  const sha = typeof record.sha256 === 'string' ? record.sha256.slice(0, 10) : 'receipt';
  const path = typeof record.artifact_path === 'string' ? record.artifact_path : '';
  return `${tab} ${sha}${path ? ` ${path}` : ''}`;
}

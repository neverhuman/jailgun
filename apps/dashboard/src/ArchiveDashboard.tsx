import { Activity } from 'lucide-react';
import { useEffect, useState } from 'react';

import { PairDashboard } from './components/PairDashboard';
import { RunHeader } from './components/RunHeader';
import { TabRow } from './components/TabRow';
import type { JailgunEvent, RunSnapshot } from './types';
import { useDashboardData, type DataSource } from './useDashboardData';

export function ArchiveDashboard({ mode = 'api' }: { mode?: DataSource }) {
  const [pairingCode, setPairingCode] = useState(() => new URLSearchParams(window.location.hash.slice(1)).get('pair'));
  useEffect(() => {
    const changed = () => {
      const code = new URLSearchParams(window.location.hash.slice(1)).get('pair');
      if (code !== null) setPairingCode(code);
    };
    window.addEventListener('hashchange', changed);
    return () => window.removeEventListener('hashchange', changed);
  }, []);
  const {
    loading,
    authenticationRequired,
    refresh,
    runs,
    selectedRun: activeRun,
    receipts,
    events,
    connection,
    dataSource,
    error,
    lastEventAt
  } = useDashboardData(mode);

  if (authenticationRequired || pairingCode !== null) return <PairDashboard onPaired={async () => {
    await refresh();
    setPairingCode(null);
  }} />;

  return (
    <main className="shell">
      {!activeRun ? <h1>Jailgun</h1> : null}
      {mode === 'fixture' ? <p className="notice" role="status">Demo mode — synthetic data</p> : null}
      {error ? <div className="notice errorState" role="alert">{error}</div> : null}

      {loading ? <p role="status">Loading runs…</p> : !activeRun ? (
        <section className="emptyState" aria-label="empty state">
          <Activity size={28} />
          <h2>No runs yet</h2>
          <p>Waiting for run snapshots.</p>
        </section>
      ) : (
        <>
          <RunHeader run={activeRun} connection={connection} dataSource={dataSource} events={events} />

          <section className="tabList" aria-label="tabs">
            {activeRun.tabs.length === 0 ? (
              <p className="muted">No tabs launched yet for this run.</p>
            ) : (
              activeRun.tabs.map((tab) => (
                <TabRow
                  key={tab.tab_id}
                  tab={tab}
                  events={events}
                  receipts={receipts}
                  lastEventAt={lastEventAt[tab.tab_id]}
                />
              ))
            )}
          </section>

          {runs.length > 1 ? (
            <section className="otherRuns" aria-label="other runs">
              <h2>Other runs</h2>
              <RunsTable runs={runs} activeRunId={activeRun.run_id} />
            </section>
          ) : null}
        </>
      )}
    </main>
  );
}

function RunsTable({ runs, activeRunId }: { runs: RunSnapshot[]; activeRunId: string }) {
  const others = runs.filter((run) => run.run_id !== activeRunId);
  if (others.length === 0) return null;
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
        {others.map((run) => (
          <tr key={run.run_id}>
            <td>{run.run_id}</td>
            <td>{run.status}</td>
            <td>{run.tabs.length}</td>
            <td>{run.deploy_queue}</td>
            <td>
              {run.denied_github_prompts} denied / {run.allowed_info_prompts} info
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

// Used by tests that import the run-level event slot helper.
export type { JailgunEvent };

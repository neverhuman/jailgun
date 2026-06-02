import { useEffect, useMemo, useState } from 'react';

import { fetchHistory } from '../api';
import type { RunHistoryEntry } from '../types';
import { MiniChart } from './MiniChart';
import type { MiniChartDatum } from './MiniChart';

export interface HistoryPanelProps {
  hasActiveRun: boolean;
}

export function HistoryPanel({ hasActiveRun }: HistoryPanelProps) {
  const [history, setHistory] = useState<RunHistoryEntry[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [expanded, setExpanded] = useState(!hasActiveRun);

  useEffect(() => {
    let ignore = false;
    setLoading(true);
    fetchHistory()
      .catch(() => fetchHistory({ mode: 'fixture' }))
      .then((entries) => {
        if (!ignore) {
          setHistory(entries);
          setLoading(false);
        }
      });
    return () => {
      ignore = true;
    };
  }, []);

  const stats = useMemo(() => {
    if (!history || history.length === 0) return null;
    const totalRuns = history.length;
    const avgSuccess =
      history.reduce((sum, entry) => {
        const rate = entry.total_tabs > 0 ? (entry.tabs_passed / entry.total_tabs) * 100 : 0;
        return sum + rate;
      }, 0) / totalRuns;
    const totalDeploys = history.reduce((sum, entry) => sum + entry.tabs_pushed, 0);
    return { totalRuns, avgSuccess: Math.round(avgSuccess), totalDeploys };
  }, [history]);

  const successRateData: MiniChartDatum[] = useMemo(() => {
    if (!history) return [];
    return history.map((entry) => ({
      label: entry.run_id,
      value: entry.total_tabs > 0 ? Math.round((entry.tabs_passed / entry.total_tabs) * 100) : 0
    }));
  }, [history]);

  const codeChangesData: MiniChartDatum[] = useMemo(() => {
    if (!history) return [];
    return history.map((entry) => ({
      label: entry.run_id,
      value: entry.code_stats?.total_additions ?? 0
    }));
  }, [history]);

  const deployCadenceData: MiniChartDatum[] = useMemo(() => {
    if (!history) return [];
    const byDay = new Map<string, number>();
    for (const entry of history) {
      const day = entry.started_at.slice(0, 10);
      byDay.set(day, (byDay.get(day) ?? 0) + 1);
    }
    return Array.from(byDay.entries())
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([day, count]) => ({ label: day, value: count }));
  }, [history]);

  return (
    <section className="historyPanel" aria-label="history panel">
      <button
        className="historyToggle"
        onClick={() => setExpanded((v) => !v)}
        aria-expanded={expanded}
      >
        <span className="historyChevron">{expanded ? '▾' : '▸'}</span>
        Run History
      </button>

      {expanded ? (
        loading ? (
          <p className="muted">Loading history...</p>
        ) : !history || history.length === 0 ? (
          <p className="muted">No history available.</p>
        ) : (
          <>
            {stats ? (
              <div className="historySummary">
                <div className="historyStat">
                  <span className="historyStatLabel">Total Runs</span>
                  <strong className="historyStatValue">{stats.totalRuns}</strong>
                </div>
                <div className="historyStat">
                  <span className="historyStatLabel">Avg Success Rate</span>
                  <strong className="historyStatValue">{stats.avgSuccess}%</strong>
                </div>
                <div className="historyStat">
                  <span className="historyStatLabel">Total Deploys</span>
                  <strong className="historyStatValue">{stats.totalDeploys}</strong>
                </div>
              </div>
            ) : null}

            <div className="historyCharts">
              <div className="chartContainer">
                <h3>Success Rate</h3>
                <MiniChart data={successRateData} type="line" color="#2f7d42" />
              </div>
              <div className="chartContainer">
                <h3>Code Changes</h3>
                <MiniChart data={codeChangesData} type="bar" color="#4a7c59" />
              </div>
              <div className="chartContainer">
                <h3>Deploy Cadence</h3>
                <MiniChart data={deployCadenceData} type="bar" color="#5a8bbf" />
              </div>
            </div>
          </>
        )
      ) : null}
    </section>
  );
}

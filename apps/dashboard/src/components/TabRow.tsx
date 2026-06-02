import { ChevronDown, ChevronRight, Lock } from 'lucide-react';
import { useState } from 'react';

import type { JailgunEvent, TabSnapshot } from '../types';
import { TabDrilldown } from './TabDrilldown';
import { TabProgressBar } from './TabProgressBar';
import { isTabClosed, isTabFailed, isTabPassed, summarizeOutcome, type OutcomeSummary } from './stages';

interface TabRowProps {
  tab: TabSnapshot;
  tabCount: number;
  events: JailgunEvent[];
  receipts: unknown[];
  lastEventAt: number | undefined;
  initialExpanded?: boolean;
}

export function TabRow({ tab, tabCount, events, receipts, lastEventAt, initialExpanded = false }: TabRowProps) {
  const [expanded, setExpanded] = useState(initialExpanded);
  const closed = isTabClosed(tab);
  const failed = isTabFailed(tab);
  const passed = isTabPassed(tab);
  const summary = summarizeOutcome(events, tab.tab_id);
  const outcomeLabel = failed
    ? summary.outcome || 'failed'
    : passed
      ? summary.outcome === 'succeeded-ci-skipped'
        ? 'succeeded'
        : summary.outcome || 'passed'
      : tab.deploy_status || tab.status || 'pending';
  const latency = tab.download_latency_ms ? `${tab.download_latency_ms} ms` : null;
  const profileLabel = tab.browser_profile
    ? `${tab.browser_profile}${tab.browser_slot ? ` #${tab.browser_slot}` : ''}`
    : tab.browser_slot
      ? `profile #${tab.browser_slot}`
      : null;

  return (
    <article
      className={`tabRow ${failed ? 'failed' : passed ? 'passed' : ''}`}
      aria-label={`tab ${tab.tab_id} of ${tabCount} row`}
    >
      <div className="tabRowMain">
        <button
          type="button"
          className="tabRowToggle"
          onClick={() => setExpanded((current) => !current)}
          aria-expanded={expanded}
          aria-controls={`tab-${tab.tab_id}-drilldown`}
          aria-label={expanded ? `collapse tab ${tab.tab_id}` : `expand tab ${tab.tab_id}`}
        >
          {expanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
        </button>
        <div className="tabRowIdent">
          <span className="tabRowNumber">
            Tab {tab.tab_id} of {tabCount}
          </span>
          {closed ? (
            <span className="closedPill" aria-label="tab closed">
              <Lock size={11} /> Closed
            </span>
          ) : null}
          {profileLabel ? <span className="tabRowProfile" aria-label="browser profile">{profileLabel}</span> : null}
          {latency ? <span className="tabRowLatency">{latency}</span> : null}
        </div>
        <TabProgressBar tab={tab} events={events} lastEventAt={lastEventAt} />
        <FinishStatePill passed={passed} failed={failed} summary={summary} />
        <span className={`tabRowOutcome ${failed ? 'failed' : passed ? 'passed' : ''}`} aria-label="tab outcome">
          {outcomeLabel}
        </span>
      </div>
      {expanded ? (
        <div id={`tab-${tab.tab_id}-drilldown`}>
          <TabDrilldown tab={tab} events={events} receipts={receipts} />
        </div>
      ) : null}
    </article>
  );
}

interface FinishStatePillProps {
  passed: boolean;
  failed: boolean;
  summary: OutcomeSummary;
}

function FinishStatePill({ passed, failed, summary }: FinishStatePillProps) {
  if (passed) {
    const sha = summary.postHead ? summary.postHead.slice(0, 10) : null;
    return (
      <span className="finishStatePill passed" aria-label="tab outcome state">
        FINISHED{sha ? <> · <code>{sha}</code></> : null}
      </span>
    );
  }
  if (failed && summary.outcome === 'failed-preserved') {
    return (
      <span className="finishStatePill preserved" aria-label="tab outcome state">
        PRESERVED
      </span>
    );
  }
  if (failed) {
    return (
      <span className="finishStatePill failed" aria-label="tab outcome state">
        FAILED
      </span>
    );
  }
  return null;
}

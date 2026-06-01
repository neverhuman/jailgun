import { CheckCircle2, XCircle } from 'lucide-react';
import { useMemo, useState } from 'react';

import type { JailgunEvent, TabSnapshot } from '../types';
import { FailureTooltip } from './FailureTooltip';
import { deriveStages, summarizeOutcome, isTabFailed, isTabPassed } from './stages';

interface TabProgressBarProps {
  tab: TabSnapshot;
  events: JailgunEvent[];
  lastEventAt: number | undefined;
  flashWindowMs?: number;
}

export function TabProgressBar({ tab, events, lastEventAt, flashWindowMs = 900 }: TabProgressBarProps) {
  const stages = useMemo(() => deriveStages(tab), [tab]);
  const summary = useMemo(() => summarizeOutcome(events, tab.tab_id), [events, tab.tab_id]);
  const failed = isTabFailed(tab);
  const passed = isTabPassed(tab);
  const [showFailureTooltip, setShowFailureTooltip] = useState(false);

  const now = Date.now();
  const flashing =
    typeof lastEventAt === 'number' && now - lastEventAt < flashWindowMs && stages[0].status === 'active';

  return (
    <div
      className="progressBar"
      role="group"
      aria-label={`tab ${tab.tab_id} progress`}
      data-passed={passed ? 'true' : undefined}
      data-failed={failed ? 'true' : undefined}
    >
      {stages.map((stage, index) => {
        const flashClass = stage.key === 'polling' && flashing ? ' flash' : '';
        return (
          <div
            key={`${stage.key}-${lastEventAt ?? 0}-${index}`}
            className={`progressSegment ${stage.status}${flashClass}`}
            title={`${stage.label}: ${stage.detail}`}
            role="presentation"
          >
            <span className="progressLabel">{stage.label}</span>
            {stage.key === 'outcome' && stage.status === 'done' ? (
              <CheckCircle2 size={14} aria-label="passed" />
            ) : null}
            {stage.key === 'outcome' && stage.status === 'failed' ? (
              <button
                type="button"
                className="progressFailureButton"
                aria-label="show failure trace"
                onMouseEnter={() => setShowFailureTooltip(true)}
                onMouseLeave={() => setShowFailureTooltip(false)}
                onFocus={() => setShowFailureTooltip(true)}
                onBlur={() => setShowFailureTooltip(false)}
              >
                <XCircle size={14} aria-label="failed" />
              </button>
            ) : null}
          </div>
        );
      })}
      {failed ? <FailureTooltip summary={summary} visible={showFailureTooltip} /> : null}
    </div>
  );
}

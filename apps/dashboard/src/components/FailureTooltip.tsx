import type { ReactNode } from 'react';

import type { OutcomeSummary } from './stages';

interface FailureTooltipProps {
  summary: OutcomeSummary;
  visible: boolean;
  children?: ReactNode;
}

export function FailureTooltip({ summary, visible }: FailureTooltipProps) {
  if (!visible) {
    return null;
  }
  const headerLines: string[] = [];
  if (summary.outcome) headerLines.push(`outcome=${summary.outcome}`);
  if (summary.exitCode) headerLines.push(`exit_code=${summary.exitCode}`);
  if (summary.remoteCommand) headerLines.push(`remote_command=${summary.remoteCommand}`);
  if (summary.remoteTarget) headerLines.push(`remote_target=${summary.remoteTarget}`);
  return (
    <div className="failureTooltip" role="tooltip" aria-label="failure trace">
      {headerLines.length > 0 ? (
        <pre className="failureTooltipHeader">{headerLines.join('\n')}</pre>
      ) : null}
      {summary.logTail ? (
        <pre className="failureTooltipBody">{summary.logTail}</pre>
      ) : (
        <p className="muted">No log tail recorded.</p>
      )}
    </div>
  );
}

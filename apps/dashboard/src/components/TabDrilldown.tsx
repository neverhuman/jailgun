import { ExternalLink } from 'lucide-react';
import { useMemo, useState } from 'react';

import type { JailgunEvent, TabSnapshot } from '../types';
import { summarizeOutcome } from './stages';

interface TabDrilldownProps {
  tab: TabSnapshot;
  events: JailgunEvent[];
  receipts: unknown[];
}

const RECENT_EVENT_LIMIT = 10;

export function TabDrilldown({ tab, events, receipts }: TabDrilldownProps) {
  const summary = useMemo(() => summarizeOutcome(events, tab.tab_id), [events, tab.tab_id]);
  const tabEvents = useMemo(
    () => events.filter((event) => event.tab_id === tab.tab_id).slice(0, RECENT_EVENT_LIMIT),
    [events, tab.tab_id]
  );
  const tabReceipts = useMemo(() => {
    return receipts.filter((receipt) => {
      if (typeof receipt !== 'object' || receipt === null) return false;
      const record = receipt as Record<string, unknown>;
      return record.tab_id === tab.tab_id;
    });
  }, [receipts, tab.tab_id]);
  const [logExpanded, setLogExpanded] = useState(false);

  return (
    <div className="tabDrilldown" role="region" aria-label={`tab ${tab.tab_id} detail`}>
      <div className="tabDrilldownGrid">
        <DrilldownField label="Conversation">
          {tab.page_url ? (
            <a
              href={tab.page_url}
              target="_blank"
              rel="noreferrer"
              className="conversationLink"
              aria-label={`open conversation for tab ${tab.tab_id}`}
            >
              {extractConversationId(tab.page_url)}
              <ExternalLink size={12} />
            </a>
          ) : (
            <span className="muted">unknown URL</span>
          )}
        </DrilldownField>
        <DrilldownField label="Local sha256">
          <code>{summary.localSha ?? tab.archive_sha256 ?? '—'}</code>
        </DrilldownField>
        <DrilldownField label="Remote sha256">
          <code>{summary.remoteSha ?? '—'}</code>
        </DrilldownField>
        <DrilldownField label="Outcome">
          <code>{summary.outcome || tab.deploy_status || 'pending'}</code>
        </DrilldownField>
        <DrilldownField label="Remote command">
          <code>{summary.remoteCommand ?? '—'}</code>
        </DrilldownField>
        <DrilldownField label="Remote target">
          <code>{summary.remoteTarget ?? '—'}</code>
        </DrilldownField>
        <DrilldownField label="Post commit">
          <code>{summary.postHead ? summary.postHead.slice(0, 12) : '—'}</code>
        </DrilldownField>
        <DrilldownField label="Local CI">
          <code>{localCiLabel(summary)}</code>
        </DrilldownField>
        <DrilldownField label="Download latency">
          <code>{tab.download_latency_ms ? `${tab.download_latency_ms} ms` : '—'}</code>
        </DrilldownField>
        <DrilldownField label="Policy">
          <code>{tab.prompt_policy_decision ?? 'none'}</code>
        </DrilldownField>
      </div>

      <section aria-label="files changed">
        <h4>Files changed ({summary.filesChanged.length})</h4>
        {summary.shortstat ? <p className="muted">{summary.shortstat}</p> : null}
        {summary.filesChanged.length === 0 ? (
          <p className="muted">No files reported yet.</p>
        ) : (
          <ul className="filesChangedList">
            {summary.filesChanged.map((file) => (
              <li key={file}>
                <code>{file}</code>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section aria-label="remote git status">
        <h4>Git status</h4>
        <div className="gitStatusGrid">
          <StatusList label="Before" items={summary.preStatus} />
          <StatusList label="After" items={summary.postStatus} />
        </div>
      </section>

      {summary.logTail ? (
        <section aria-label="remote log tail">
          <button
            type="button"
            className="logToggleButton"
            onClick={() => setLogExpanded((current) => !current)}
            aria-expanded={logExpanded}
          >
            {logExpanded ? 'Hide remote log' : 'Show remote log'}
          </button>
          {logExpanded ? <pre className="logTail">{summary.logTail}</pre> : null}
        </section>
      ) : null}

      <section aria-label="recent events">
        <h4>Recent events ({tabEvents.length})</h4>
        {tabEvents.length === 0 ? (
          <p className="muted">No events for this tab yet.</p>
        ) : (
          <ul className="tabEventList">
            {tabEvents.map((event, index) => (
              <li key={`${event.timestamp}-${index}`}>
                <span className={`severity ${event.severity}`}>{event.severity}</span>
                <span className="eventKind">{event.kind}</span>
                <span className="eventMessage">{event.message}</span>
              </li>
            ))}
          </ul>
        )}
      </section>

      {tabReceipts.length > 0 ? (
        <section aria-label="receipts for this tab">
          <h4>Receipts ({tabReceipts.length})</h4>
          <ul className="tabReceiptList">
            {tabReceipts.map((receipt, index) => (
              <li key={index}>
                <code>{formatReceipt(receipt)}</code>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </div>
  );
}

function DrilldownField({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="tabDrilldownField">
      <span className="tabDrilldownLabel">{label}</span>
      <span className="tabDrilldownValue">{children}</span>
    </div>
  );
}

function StatusList({ label, items }: { label: string; items: string[] }) {
  return (
    <div className="statusList">
      <span className="tabDrilldownLabel">{label}</span>
      {items.length === 0 ? (
        <code>clean</code>
      ) : (
        <ul className="filesChangedList">
          {items.map((item) => (
            <li key={item}>
              <code>{item}</code>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function localCiLabel(summary: ReturnType<typeof summarizeOutcome>): string {
  const log = summary.logTail ?? '';
  if (
    summary.remoteCommand?.includes('ci-fast-push') &&
    log.includes('ci-fast-push: jekko-fast passed') &&
    /cargo test:\s+\d+ passed/.test(log)
  ) {
    return 'xbabe2 passed';
  }
  if (summary.ciState === 'passed') return 'GitHub passed';
  if (summary.ciState === 'skipped') return 'GitHub skipped';
  return summary.ciState ?? 'pending';
}

function extractConversationId(url: string): string {
  const match = url.match(/\/c\/([^/?#]+)/);
  return match ? match[1] : url;
}

function formatReceipt(receipt: unknown): string {
  if (typeof receipt !== 'object' || receipt === null) {
    return String(receipt);
  }
  const record = receipt as Record<string, unknown>;
  const sha = typeof record.sha256 === 'string' ? record.sha256.slice(0, 12) : 'receipt';
  const path = typeof record.artifact_path === 'string' ? record.artifact_path : '';
  const recordedAt = typeof record.recorded_at === 'string' ? record.recorded_at : '';
  return `${sha} ${path}${recordedAt ? ` · ${recordedAt}` : ''}`.trim();
}

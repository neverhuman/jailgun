import type { AccountReadiness, Attempt, Event, Run } from '../generated/workflow';
import type { AccountAction } from './Accounts';

const STAGES = [['explore', 'Explore'], ['compare', 'Compare'], ['synthesize', 'Synthesize'], ['critique', 'Critique'], ['revise', 'Revise']] as const;
const ACTIVE = new Set(['prepared', 'submitting', 'accepted', 'uncertain']);
export function RunProgress({ run, attempts, events, account, busy, act, showResults }: {
  run: Run; attempts: Attempt[]; events: Event[]; account?: AccountReadiness; busy: boolean; act: AccountAction; showResults: () => void;
}) {
  const candidates = run.tasks.filter(task => task.stage === 'explore');
  const complete = candidates.filter(task => task.status === 'completed').length;
  const active = attempts.filter(attempt => ACTIVE.has(attempt.state));
  const stopped = ['completed', 'failed', 'cancelled'].includes(run.status);
  const incomplete = candidates.some(task => ['failed', 'partial'].includes(task.status));
  const canSubset = run.status === 'paused' && incomplete && complete >= 3 && active.length === 0 && run.tasks.filter(t => t.stage !== 'explore').every(t => t.status === 'queued');
  const path = `/api/runs/${encodeURIComponent(run.id)}`;
  return <section aria-labelledby="progress-title" className="conceptPanel">
    <div className="cardHeading"><div><p className="eyebrow">{run.request.account_id}</p><h2 id="progress-title">Run progress</h2></div><span className={`statusPill status-${run.status}`} role="status">{run.status.replaceAll('-', ' ')}</span></div>
    <p className="conceptExcerpt">{run.request.concept}</p><p className="subtle">Run {run.id}</p>
    <ol className="workflowStages" aria-label="Workflow stages">{STAGES.map(([stage, label]) => {
      const tasks = run.tasks.filter(task => task.stage === stage);
      const finished = tasks.filter(task => task.status === 'completed').length;
      const excluded = tasks.filter(task => task.status === 'excluded').length;
      return <li key={stage} className={finished > 0 && finished + excluded === tasks.length ? 'stageCompleted' : ''}><strong>{label}</strong><span>{finished} of {tasks.length} complete{excluded ? ` · ${excluded} excluded` : ''}</span></li>;
    })}</ol>
    <dl className="runFacts"><div><dt>Candidates</dt><dd>{complete} of {candidates.length} complete</dd></div><div><dt>Submissions</dt><dd>{run.submissions} used · {run.submission_limit - run.submissions} remaining</dd></div>
      <div><dt>Deadline</dt><dd>{new Date(run.deadline_ms).toLocaleString()}</dd></div><div><dt>Account capacity</dt><dd>{account ? `${account.active} / ${account.capacity} active` : 'Checking…'}</dd></div></dl>
    {!stopped && account && Math.max(account.cooldown_until_ms, account.next_submit_ms) > Date.now() ? <p role="status">{account.cooldown_until_ms > Date.now() ? 'Provider cooldown' : 'Next paced submission'}: {new Date(Math.max(account.cooldown_until_ms, account.next_submit_ms)).toLocaleTimeString()}.</p> : null}
    {run.pause_reason ? <p className="notice" role="status">{run.pause_reason.replaceAll('-', ' ')}. Completed and partial results remain available.</p> : null}
    {run.status === 'waiting-for-auth' ? <p>Reconnect this account in Accounts. Completed candidates will be retained.</p> : null}
    {run.status === 'reconciliation-required' ? <p>Submission or capture could not be confirmed. Inspect the owned conversation links before retrying; Jailgun will not submit these prompts again automatically.</p> : null}
    <div className="actions">{run.status === 'completed' ? <button className="primaryButton" onClick={showResults}>Read final concept</button> : null}
      {['queued', 'running', 'waiting-for-auth'].includes(run.status) ? <button className="secondaryButton" disabled={busy} onClick={() => void act(`${path}/pause`)}>Pause new submissions</button> : null}
      {run.status === 'paused' ? <button className="primaryButton" disabled={busy} onClick={() => void act(`${path}/resume`)}>{incomplete ? 'Retry failed candidates' : 'Resume run'}</button> : null}
      {canSubset ? <button className="secondaryButton" disabled={busy} onClick={() => void act(`${path}/resume`, { allow_incomplete: true })}>Continue with {complete} complete candidates</button> : null}
      {!stopped ? <button className="quietButton" disabled={busy} onClick={() => void act(`${path}/cancel`)}>Cancel run and retain results</button> : null}</div>
    <div className="candidateGrid">{[...run.tasks].sort((a, b) => STAGES.findIndex(([s]) => s === a.stage) - STAGES.findIndex(([s]) => s === b.stage) || a.position - b.position).map(task => {
      const history = attempts.filter(attempt => attempt.task_id === task.id).sort((a, b) => b.number - a.number);
      const attempt = history[0];
      return <article className="candidateCard" key={task.id}><div className="cardHeading"><h3>{task.stage === 'explore' ? `${task.position}. ${run.configuration.perspectives[task.position - 1]}` : STAGES.find(([stage]) => stage === task.stage)?.[1]}</h3><span className={`statusPill status-${task.status}`}>{task.status}</span></div>
        {task.summary ? <p>{task.summary.proposal}</p> : null}
        {task.error_code ? <p className="errorState">{task.error_code.replaceAll('-', ' ')}</p> : null}
        {attempt ? <p className="subtle">Attempt {attempt.number} · {attempt.state.replaceAll('-', ' ')}{attempt.observed_model ? ` · ${attempt.observed_model}` : ''}</p> : null}
        {attempt?.accepted_ms != null && ACTIVE.has(attempt.state) ? <p className="subtle">Response timeout: {new Date(attempt.accepted_ms + run.configuration.response_timeout_ms).toLocaleTimeString()}</p> : null}
        {history.some(item => item.conversation_url) ? <details><summary>Conversation history</summary><ul>{history.map(item => {
          const url = safeConversation(item.conversation_url);
          return url ? <li key={item.id}><a href={url} target="_blank" rel="noopener noreferrer">Attempt {item.number} · {item.state}</a></li> : null;
        })}</ul></details> : null}
      </article>;
    })}</div>
    {events.length ? <details className="activity"><summary>Recent activity</summary><ol>{events.map(event => <li key={event.sequence}><time dateTime={Number.isFinite(new Date(event.created_ms).getTime()) ? new Date(event.created_ms).toISOString() : undefined}>{new Date(event.created_ms).toLocaleTimeString()}</time> {event.kind.replaceAll('-', ' ')}</li>)}</ol></details> : null}
  </section>;
}
function safeConversation(value: string | null | undefined): string | null {
  if (!value) return null;
  try { const url = new URL(value); return url.protocol === 'https:' && url.hostname === 'chatgpt.com' && url.pathname.startsWith('/c/') && !url.username && !url.password ? url.href : null; } catch { return null; }
}

import { useEffect, useRef, useState } from 'react';
import { PairDashboard } from '../components/PairDashboard';
import type { ConceptRequest } from '../generated/workflow';
import { Accounts } from './Accounts';
import { NewConcept, emptyDraft } from './NewConcept';
import { RunProgress } from './RunProgress';
import { Results } from './Results';
import { Tokens } from './Tokens';
import { useConceptData } from './useConceptData';
import { demoData, demoArtifacts } from './demo';
import * as api from './api';

const PAGES = [['accounts', 'Accounts'], ['new', 'New concept'], ['runs', 'Run progress'], ['results', 'Results']] as const;
type Page = typeof PAGES[number][0];
function currentPage(): Page {
  const hash = window.location.hash.slice(1);
  return PAGES.some(([id]) => id === hash) ? hash as Page : 'accounts';
}
export function ConceptDashboard({ demo = false }: { demo?: boolean }) {
  const data = useConceptData(demo ? demoData : undefined);
  const [page, setPage] = useState(currentPage);
  const [draft, setDraft] = useState(emptyDraft);
  const [busy, setBusy] = useState(false);
  const [pairingCode, setPairingCode] = useState(() => new URLSearchParams(window.location.hash.slice(1)).get('pair'));
  const pending = useRef<{ content: string; key: string } | null>(null);
  const acting = useRef(false);
  useEffect(() => {
    function changed() {
      const code = new URLSearchParams(window.location.hash.slice(1)).get('pair');
      if (code !== null) setPairingCode(code);
      else if (PAGES.some(([id]) => id === window.location.hash.slice(1))) setPage(currentPage());
    }
    window.addEventListener('hashchange', changed);
    return () => window.removeEventListener('hashchange', changed);
  }, []);
  function navigate(next: Page) { setPage(next); window.location.hash = next; }
  async function act(path: string, body?: unknown): Promise<boolean> {
    if (acting.current || demo) return false;
    acting.current = true; setBusy(true); data.clearError();
    try { await api.mutate(path, body); await data.refresh(); return true; }
    catch (cause) { data.report(cause); return false; }
    finally { acting.current = false; setBusy(false); }
  }
  async function submit(request: Omit<ConceptRequest, 'idempotency_key'>) {
    if (acting.current || demo) return;
    acting.current = true; setBusy(true); data.clearError();
    const content = JSON.stringify(request);
    if (pending.current?.content !== content) pending.current = { content, key: crypto.randomUUID() };
    try {
      const accepted = await api.request('run_accepted', '/api/concept-runs', 'POST', { ...request, idempotency_key: pending.current.key });
      pending.current = null; data.selectRun(accepted.run_id); navigate('runs'); await data.refresh();
    } catch (cause) { data.report(cause); }
    finally { acting.current = false; setBusy(false); }
  }
  if (!demo && (data.authenticationRequired || pairingCode !== null)) return <PairDashboard onPaired={async () => {
    await data.refresh(); setPairingCode(null); setPage(currentPage());
  }} />;
  const run = data.run?.id === data.selectedId ? data.run : null;
  return <main className="shell conceptShell">
    <a className="skipLink" href="#workspace">Skip to workspace</a>
    <header className="conceptHeader"><div><h1>Jailgun</h1><p>One concept. Different perspectives. A considered result.</p></div><span className="localLabel">Private installation</span></header>
    {demo ? <p className="notice demoNotice" role="status">Demo mode — synthetic data. No ChatGPT account or live generation is used.</p> : null}
    <nav className="conceptNav" aria-label="Main navigation">{PAGES.map(([id, label]) => <a key={id} href={`#${id}`} aria-current={page === id ? 'page' : undefined}>{label}</a>)}</nav>
    <div id="workspace" tabIndex={-1}>
      {data.error ? <div className="notice errorState" role="alert">{data.updated === null ? <h2>Unable to load your installation</h2> : null}<p>{api.explain(data.error)}</p><button className="secondaryButton" onClick={() => void data.refresh()}>Reconnect to service</button></div> : null}
      {busy ? <p className="operationStatus" role="status">Saving your action…</p> : null}
      {data.loading ? <p role="status">Loading your installation…</p> : data.updated === null && data.error && !demo ? null : <>
        {page === 'accounts' ? <><Accounts sessions={data.sessions} readiness={data.accounts} busy={busy} act={act} demo={demo} /><Tokens accounts={data.accounts} report={data.report} demo={demo} /></> : null}
        {page === 'new' ? <NewConcept draft={draft} setDraft={setDraft} accounts={data.accounts} busy={busy} submit={submit} demo={demo} /> : null}
        {page === 'runs' || page === 'results' ? <>
          {data.runs.length ? <div className="runSelector"><label htmlFor="selected-run">Selected run</label><select id="selected-run" value={data.selectedId ?? ''} onChange={event => data.selectRun(event.target.value)}>
            {data.runs.map(item => <option key={item.id} value={item.id}>{item.request.concept.slice(0, 90)} · {item.status} · {item.id}</option>)}
          </select></div> : <section className="emptyState"><h2>No concepts yet</h2><p>Start with an idea or question. Your candidates and final concept will be kept here.</p><button className="primaryButton" onClick={() => navigate(data.accounts.some(a => a.readiness === 'ready') ? 'new' : 'accounts')}>{data.accounts.some(a => a.readiness === 'ready') ? 'Explore a concept' : 'Connect an account'}</button></section>}
          {run && page === 'runs' ? <RunProgress run={run} attempts={data.attempts} events={data.events} account={data.accounts.find(a => a.id === run.request.account_id)} busy={busy || demo} act={act} showResults={() => navigate('results')} /> : null}
          {run && page === 'results' ? <Results key={run.id} run={run} attempts={data.attempts} report={data.report} demoArtifacts={demo ? demoArtifacts : undefined} /> : null}
          {!run && data.selectedId ? <p role="status">Loading selected run…</p> : null}
        </> : null}
      </>}
    </div>
    <footer className="conceptFooter"><p>Results stay on this installation. Complete, partial and excluded responses are identified separately.</p><a href={demo ? '?advanced=1&demo=1' : '?advanced=1'} target="_blank" rel="noopener noreferrer">Advanced archive workflows</a></footer>
  </main>;
}

import { useState, type FormEvent } from 'react';
import { DEFAULT_CRITERIA } from '../generated/workflow-defaults';
import type { AccountReadiness, ConceptRequest } from '../generated/workflow';
import { validate } from './api';

export interface ConceptDraft {
  concept: string; constraints: string; account: string; count: number;
  criteria: { name: string; weight: string }[];
}
export function emptyDraft(): ConceptDraft {
  return { concept: '', constraints: '', account: '', count: 5, criteria: DEFAULT_CRITERIA.map(c => ({ name: c.name, weight: String(c.weight) })) };
}
export function NewConcept({ draft, setDraft, accounts, busy, submit, demo }: {
  draft: ConceptDraft; setDraft: (draft: ConceptDraft) => void; accounts: AccountReadiness[];
  busy: boolean; submit: (request: Omit<ConceptRequest, 'idempotency_key'>) => Promise<void>; demo: boolean;
}) {
  const [validation, setValidation] = useState('');
  const selected = accounts.find(account => account.id === draft.account);
  const total = draft.criteria.reduce((sum, c) => sum + Number(c.weight), 0);
  async function launch(event: FormEvent) {
    event.preventDefault(); setValidation('');
    if (total !== 100 || new Set(draft.criteria.map(c => c.name.trim().toLowerCase())).size !== draft.criteria.length) {
      setValidation('Use unique criteria with positive whole-number weights totaling 100.'); return;
    }
    const request = { concept: draft.concept.trim(), constraints: draft.constraints.trim(), account_id: draft.account,
      candidate_count: draft.count, criteria: draft.criteria.map(c => ({ name: c.name.trim(), weight: Number(c.weight) })) };
    try { validate('concept_request', { ...request, idempotency_key: 'validation-only' }); }
    catch { setValidation('Check the concept, account, candidate count and criteria before launching.'); return; }
    await submit(request);
  }
  return <section aria-labelledby="new-concept-title" className="conceptPanel">
    <p className="eyebrow">Start with a question worth exploring</p>
    <h2 id="new-concept-title">New concept</h2>
    <p className="subtle">Explore different perspectives, compare the candidates, then synthesize, critique and revise a final concept.</p>
    {accounts.length === 0 ? <p className="notice">Connect ChatGPT in Accounts to start your first exploration.</p> : null}
    <form className="conceptForm" onSubmit={launch}>
      <label htmlFor="concept-text">What do you want to explore?</label>
      <p className="subtle" id="concept-help">Describe the idea, problem, or question and the outcome you want.</p>
      <textarea aria-describedby="concept-help" id="concept-text" rows={6} maxLength={64000} required value={draft.concept} onChange={event => setDraft({ ...draft, concept: event.target.value })} />
      <label htmlFor="concept-constraints">Constraints and context <span className="subtle">(optional)</span></label>
      <p className="subtle" id="constraints-help">Who is this for? What budget, evidence, or limitations should every candidate consider?</p>
      <textarea aria-describedby="constraints-help" id="concept-constraints" rows={3} maxLength={16000} value={draft.constraints} onChange={event => setDraft({ ...draft, constraints: event.target.value })} />
      <div className="formColumns"><div>
        <label htmlFor="concept-account">ChatGPT account</label>
        <select id="concept-account" required value={draft.account} onChange={event => setDraft({ ...draft, account: event.target.value })}>
          <option value="">Choose an account</option>
          {accounts.map(account => <option key={account.id} value={account.id}>{account.id} · {account.readiness.replaceAll('-', ' ')}</option>)}
        </select>
      </div><div>
        <label htmlFor="concept-count">Perspectives</label>
        <select id="concept-count" value={draft.count} onChange={event => setDraft({ ...draft, count: Number(event.target.value) })}>
          {[5, 6, 7, 8, 9, 10].map(count => <option key={count} value={count}>{count} candidates</option>)}
        </select>
      </div></div>
      <details className="criteriaEditor"><summary>Evaluation criteria · total {total}%</summary>
        <p className="subtle">Each perspective receives the same criteria. Scores are model judgments, not independently verified facts.</p>
        {draft.criteria.map((criterion, index) => <div className="criterionRow" key={index}>
          <label>Criterion {index + 1}<input required maxLength={120} value={criterion.name} onChange={event => setDraft({ ...draft, criteria: draft.criteria.map((c, i) => i === index ? { ...c, name: event.target.value } : c) })} /></label>
          <label>Weight {index + 1} (%)<input type="number" min={1} max={100} step={1} required value={criterion.weight} onChange={event => setDraft({ ...draft, criteria: draft.criteria.map((c, i) => i === index ? { ...c, weight: event.target.value } : c) })} /></label>
          <button type="button" className="quietButton" aria-label={`Remove criterion ${index + 1}`} disabled={draft.criteria.length === 1} onClick={() => setDraft({ ...draft, criteria: draft.criteria.filter((_, i) => i !== index) })}>Remove</button>
        </div>)}
        <div className="actions"><button type="button" className="secondaryButton" disabled={draft.criteria.length >= 10} onClick={() => setDraft({ ...draft, criteria: [...draft.criteria, { name: '', weight: '1' }] })}>Add criterion</button>
          <button type="button" className="quietButton" onClick={() => setDraft({ ...draft, criteria: emptyDraft().criteria })}>Reset criteria</button></div>
      </details>
      <p className="subtle">{draft.count + 4} normal submissions, paced across the account. Up to {2 * (draft.count + 4)} submissions including bounded retries; overall deadline three hours.</p>
      {validation ? <p role="alert" className="errorState">{validation}</p> : null}
      {selected && selected.readiness !== 'ready' ? <p role="status">This account needs attention in Accounts before you can launch.</p> : null}
      {demo ? <p className="notice">Demo mode shows a synthetic example. Connect your own installation to submit a concept.</p> : null}
      <button className="primaryButton" type="submit" disabled={busy || selected?.readiness !== 'ready' || demo}>{busy ? 'Submitting…' : 'Explore this concept'}</button>
    </form>
  </section>;
}

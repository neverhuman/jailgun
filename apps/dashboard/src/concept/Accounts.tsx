import { useId, useState, type FormEvent } from 'react';
import type { AccountReadiness, AccountSession, ConfirmAccount } from '../generated/workflow';
import { LoginViewer } from './LoginViewer';

export type AccountAction = (path: string, body?: unknown) => Promise<boolean>;
export function Accounts({ sessions, readiness, busy, act, demo }: { sessions: AccountSession[]; readiness: AccountReadiness[]; busy: boolean; act: AccountAction; demo: boolean }) {
  const [email, setEmail] = useState('');
  const connecting = sessions.some(session => ['starting', 'login-required', 'waiting-for-user', 'verifying'].includes(session.lifecycle));
  async function connect(event: FormEvent) { event.preventDefault(); if (await act('/api/accounts/connect', { email: email.trim() })) setEmail(''); }
  return <section aria-labelledby="accounts-title" className="conceptPanel">
    <p className="eyebrow">Your private browser profiles</p><h2 id="accounts-title">Accounts</h2>
    <p className="subtle">Sign in once through ChatGPT's normal browser login. Jailgun keeps the profile on this installation and verifies the account again after a restart.</p>
    {sessions.length === 0 ? <div className="emptyState"><h3>Connect your first account</h3><p>Use an account you control. Complete passwords and verification in the dedicated ChatGPT browser window.</p></div> : null}
    <div className="accountList">{sessions.map(session => <AccountCard key={session.account_id} session={session} readiness={readiness.find(a => a.id === session.account_id)} busy={busy || demo} act={act} />)}</div>
    <form onSubmit={connect} className="connectForm"><label htmlFor="account-email">ChatGPT account email</label>
      <div className="actions"><input id="account-email" type="email" autoComplete="email" maxLength={254} required value={email} onChange={event => setEmail(event.target.value)} />
        <button type="submit" className={connecting ? 'secondaryButton' : 'primaryButton'} disabled={busy || demo}>Connect ChatGPT</button></div>
    </form>
  </section>;
}
function AccountCard({ session, readiness, busy, act }: { session: AccountSession; readiness?: AccountReadiness; busy: boolean; act: AccountAction }) {
  const modelId = useId();
  const [choice, setChoice] = useState<string | null>(null);
  const [viewOpen, setViewOpen] = useState(false);
  const model = choice ?? (session.model?.mode === 'specific' ? session.model.name : '');
  const observation = session.observation;
  const confirming = session.lifecycle === 'verifying';
  const loginActive = ['starting', 'login-required', 'waiting-for-user', 'verifying'].includes(session.lifecycle);
  async function confirm() {
    if (!observation) return;
    const body: ConfirmAccount = { identity: observation.identity, model: model ? { mode: 'specific', name: model } : { mode: 'current' } };
    if (await act(`/api/accounts/${encodeURIComponent(session.account_id)}/confirm`, body)) setChoice(null);
  }
  return <article className="accountCard" aria-label={`Account ${session.account_id}`}>
    <div className="cardHeading"><div><h3>{session.email}</h3><p className="subtle">{session.account_id}</p></div><span className={`statusPill status-${session.lifecycle}`}>{session.lifecycle.replaceAll('-', ' ')}</span></div>
    <p>{readiness ? `${readiness.active} of ${readiness.capacity} conversation slots in use` : 'Checking account capacity…'}</p>
    {readiness && readiness.cooldown_until_ms > Date.now() ? <p role="status">Provider cooldown until {new Date(readiness.cooldown_until_ms).toLocaleTimeString()}.</p> : null}
    {['starting', 'login-required', 'waiting-for-user'].includes(session.lifecycle) ? <p role="status">Complete ChatGPT sign-in {session.login_view_available ? 'in the private login view below' : 'in the dedicated browser window'}. Keep this page open while Jailgun verifies the account.</p> : null}
    {confirming && observation ? <div className="identityConfirmation"><p>Detected <strong>{observation.identity.email}</strong></p><p className="subtle">Account identity: {observation.identity.id}</p><p>Confirm this is the account you intend to use, and choose its model behavior.</p></div> : null}
    {observation && ['ready', 'verifying'].includes(session.lifecycle) ? <div className="modelChoice">
      <label htmlFor={modelId}>Model for new runs</label>
      <select
        id={modelId}
        aria-describedby={`${modelId}-observed`}
        value={model}
        onChange={event => setChoice(event.target.value)}
        disabled={busy}
      >
        <option value="">Use current selection</option>
        {model && ![...observation.available_models, observation.model].includes(model) ? <option value={model} disabled>{model} (unavailable)</option> : null}
        {[...new Set([...observation.available_models, observation.model])].map(name => <option key={name} value={name}>{name}</option>)}
      </select><button className={confirming ? 'primaryButton' : 'secondaryButton'} disabled={busy || (!confirming && choice === null)} onClick={() => void confirm()}>{confirming ? 'Confirm account and model' : 'Save model choice'}</button>
      <p className="subtle observedModel" id={`${modelId}-observed`}>Current browser selection: {observation.model}</p>
    </div> : null}
    {session.error_code && !(loginActive && session.error_code === 'authentication-expired') ? <p className="errorState">{session.error_code.replaceAll('-', ' ')}. Reconnect to verify the account; if the browser cannot start, run jailgun doctor.</p> : null}
    {session.login_view_available && loginActive ? <div className="serverLogin">
      {viewOpen ? <LoginViewer accountId={session.account_id} onClose={() => setViewOpen(false)} /> : <button className={confirming ? 'secondaryButton' : 'primaryButton'} disabled={busy} onClick={() => setViewOpen(true)}>Open login view</button>}
    </div> : null}
    <div className="actions">{!['ready', 'starting', 'verifying'].includes(session.lifecycle) ? <button className="secondaryButton" disabled={busy} onClick={() => void act(`/api/accounts/${encodeURIComponent(session.account_id)}/reconnect`)}>Reconnect</button> : null}
      {loginActive ? <button className="quietButton" disabled={busy} onClick={() => void act(`/api/accounts/${encodeURIComponent(session.account_id)}/cancel-login`)}>Cancel login</button> : null}</div>
  </article>;
}

import { useEffect, useState, type FormEvent } from 'react';
import type { AccountReadiness, CreatedToken, TokenMetadata } from '../generated/workflow';
import * as api from './api';

export function Tokens({ accounts, report, demo }: { accounts: AccountReadiness[]; report: (error: unknown) => void; demo: boolean }) {
  const [open, setOpen] = useState(false);
  return <details className="conceptPanel tokenPanel" onToggle={event => setOpen(event.currentTarget.open)}><summary>Automation access for CLI and MCP</summary>
    {open ? <TokenControls accounts={accounts} report={report} demo={demo} /> : null}
  </details>;
}
function TokenControls({ accounts, report, demo }: { accounts: AccountReadiness[]; report: (error: unknown) => void; demo: boolean }) {
  const [tokens, setTokens] = useState<TokenMetadata[]>([]);
  const [created, setCreated] = useState<CreatedToken | null>(null);
  const [name, setName] = useState('');
  const [selected, setSelected] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [loaded, setLoaded] = useState(demo);
  const [revision, setRevision] = useState(0);
  const [revoke, setRevoke] = useState<string | null>(null);
  function failed(cause: unknown) { setError(cause); if (cause instanceof api.WorkflowError && cause.status === 401) report(cause); }
  useEffect(() => {
    if (demo) return;
    let ignore = false;
    void api.list('token_metadata', '/api/tokens').then(value => { if (!ignore) { setTokens(value); setLoaded(true); } }).catch(cause => { if (!ignore) failed(cause); });
    return () => { ignore = true; };
  }, [demo, revision]);
  async function issue(event: FormEvent) {
    event.preventDefault(); if (busy || demo) return;
    setBusy(true); setError(null);
    try { setCreated(await api.request('created_token', '/api/tokens', 'POST', { name: name.trim(), account_ids: selected })); setName(''); setRevision(value => value + 1); }
    catch (cause) { failed(cause); }
    finally { setBusy(false); }
  }
  async function revokeToken(id: string) {
    if (busy || demo) return;
    setBusy(true); setError(null);
    try { await api.mutate(`/api/tokens/${encodeURIComponent(id)}`, undefined, 'DELETE'); if (created?.metadata.id === id) setCreated(null); setRevoke(null); setRevision(value => value + 1); }
    catch (cause) { failed(cause); }
    finally { setBusy(false); }
  }
  return <div className="tokenControls"><p>Automation tokens can submit and manage concept runs for the accounts you select. Account login and token management remain operator actions.</p>
    {error ? <div role="alert" className="errorState"><p>{api.explain(error)}</p><button className="secondaryButton" onClick={() => { setError(null); setRevision(value => value + 1); }}>Reload tokens</button></div> : null}
    {created ? <div className="notice"><h3>Save this token now</h3><p>It is shown once and disappears when you leave this screen. Store it in a private credential file for your client.</p><label htmlFor="new-token">New automation token</label><textarea id="new-token" readOnly value={created.secret} autoComplete="off" spellCheck={false} rows={3} /><button className="secondaryButton" onClick={() => setCreated(null)}>I have saved the token</button></div> : null}
    <form onSubmit={issue}><label htmlFor="token-name">Token name</label><input id="token-name" required maxLength={120} value={name} onChange={event => setName(event.target.value)} />
      <fieldset><legend>Permitted accounts</legend>{accounts.length ? accounts.map(account => <label className="checkboxLabel" key={account.id}><input type="checkbox" checked={selected.includes(account.id)} onChange={event => setSelected(current => event.target.checked ? [...current, account.id] : current.filter(id => id !== account.id))} />{account.id}</label>) : <p>Connect an account before creating a token.</p>}</fieldset>
      <button className="secondaryButton" disabled={busy || demo || selected.length === 0 || created !== null}>Create automation token</button>
    </form>
    {!loaded ? <p role="status">Loading tokens…</p> : tokens.length === 0 ? <p className="subtle">No automation tokens have been created.</p> : <ul className="tokenList">{tokens.map(token => <li key={token.id}><strong>{token.name}</strong><p>{token.account_ids.join(', ')} · {token.revoked_ms == null ? 'Active' : 'Revoked'}</p>
      {token.revoked_ms == null ? revoke === token.id ? <div className="notice"><p>Revoke {token.name}? Clients using it will lose access immediately.</p><div className="actions"><button className="secondaryButton" disabled={busy} onClick={() => void revokeToken(token.id)}>Confirm revocation</button><button className="quietButton" disabled={busy} onClick={() => setRevoke(null)}>Keep token</button></div></div> : <button className="quietButton" disabled={busy} onClick={() => setRevoke(token.id)}>Revoke {token.name}</button> : null}
    </li>)}</ul>}
  </div>;
}

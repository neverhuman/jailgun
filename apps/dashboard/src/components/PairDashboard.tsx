import { useEffect, useState, type FormEvent } from 'react';

export function PairDashboard({ onPaired }: { onPaired: () => Promise<void> }) {
  const [code, setCode] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function pair(value: string) {
    setBusy(true);
    setError(null);
    try {
      const response = await fetch('/api/session/pair', {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ code: value.trim() })
      });
      if (!response.ok) throw new Error('The pairing code is invalid or expired. Use the latest code printed by the local service.');
      setCode('');
      await onPaired();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }
  useEffect(() => {
    const fragment = new URLSearchParams(window.location.hash.slice(1));
    if (!fragment.has('pair')) return;
    // Remove the one-use credential before making requests or rendering navigation.
    window.history.replaceState(null, '', `${window.location.pathname}${window.location.search}#accounts`);
    const value = fragment.get('pair') ?? '';
    if (!/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(value)) {
      setError('This pairing link is invalid. Run jailgun setup to get a new link.');
      return;
    }
    void pair(value);
  });
  async function submit(event: FormEvent) {
    event.preventDefault();
    await pair(code);
  }
  return <main className="shell pairing">
    <h1>Jailgun</h1>
    <h2>Pair this browser</h2>
    <p>Enter the one-use code printed by your local Jailgun service. It expires after five minutes.</p>
    <form onSubmit={submit}>
      <label htmlFor="pairing-code">Pairing code</label>
      <input id="pairing-code" autoComplete="off" spellCheck={false} required value={code} onChange={event => setCode(event.target.value)} />
      <button type="submit" disabled={busy}>{busy ? 'Pairing…' : 'Pair browser'}</button>
    </form>
    {error ? <p role="alert">{error}</p> : null}
  </main>;
}

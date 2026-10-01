import { useCallback, useEffect, useRef, useState } from 'react';
import type { AccountReadiness, AccountSession, Attempt, Event, Run } from '../generated/workflow';
import * as api from './api';

export interface ConceptSnapshot { accounts: AccountReadiness[]; sessions: AccountSession[]; runs: Run[]; attempts?: Attempt[]; events?: Event[] }
export function useConceptData(demoSnapshot?: ConceptSnapshot) {
  const [snapshot, setSnapshot] = useState<ConceptSnapshot>(demoSnapshot ?? { accounts: [], sessions: [], runs: [] });
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [run, setRun] = useState<Run | null>(null);
  const [attempts, setAttempts] = useState<Attempt[]>([]);
  const [events, setEvents] = useState<Event[]>([]);
  const [loading, setLoading] = useState(!demoSnapshot);
  const [authenticationRequired, setAuthenticationRequired] = useState(false);
  const [error, setError] = useState<{ source: string; cause: unknown } | null>(null);
  const [updated, setUpdated] = useState<number | null>(null);
  const selected = useRef(selectedId); selected.current = selectedId;
  const cursor = useRef({ id: selectedId, sequence: 0 });
  const generation = useRef(0);
  const mounted = useRef(true);
  const report = useCallback((cause: unknown, source = 'action') => {
    if (cause instanceof api.WorkflowError && cause.status === 401) setAuthenticationRequired(true);
    setError({ source, cause });
  }, []);
  const refresh = useCallback(async () => {
    if (demoSnapshot) { setSnapshot(demoSnapshot); setSelectedId(current => current ?? demoSnapshot.runs[0]?.id ?? null); setLoading(false); return; }
    const request = ++generation.current;
    try {
      const [accounts, sessions, runs] = await Promise.all([
        api.list('account_readiness', '/api/accounts'), api.list('account_session', '/api/accounts/sessions'), api.list('run', '/api/runs?kind=concept')
      ]);
      if (!mounted.current || request !== generation.current) return;
      setSnapshot({ accounts, sessions, runs }); setAuthenticationRequired(false); setError(current => current?.source === 'overview' || (current?.cause instanceof api.WorkflowError && current.cause.status === 401) ? null : current); setUpdated(Date.now());
      setSelectedId(current => current ?? runs[0]?.id ?? null);
    } catch (cause) { if (mounted.current && request === generation.current) report(cause, 'overview'); }
    finally { if (mounted.current && request === generation.current) setLoading(false); }
  }, [demoSnapshot, report]);
  useEffect(() => {
    mounted.current = true; let stopped = false; let timer: ReturnType<typeof setTimeout>;
    async function poll() { await refresh(); if (!stopped && !demoSnapshot) timer = setTimeout(() => void poll(), 3000); }
    void poll();
    return () => { stopped = true; mounted.current = false; clearTimeout(timer); generation.current += 1; };
  }, [refresh, demoSnapshot]);
  useEffect(() => {
    cursor.current = { id: selectedId, sequence: 0 }; setEvents([]); setAttempts([]); setRun(null);
    if (!selectedId) return;
    if (demoSnapshot) { setRun(demoSnapshot.runs.find(value => value.id === selectedId) ?? null); setAttempts(demoSnapshot.attempts?.filter(a => a.run_id === selectedId) ?? []); setEvents(demoSnapshot.events?.filter(e => e.run_id === selectedId) ?? []); return; }
    let stopped = false; let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const after = cursor.current.id === selectedId ? cursor.current.sequence : 0;
        const [nextRun, nextAttempts, nextEvents] = await Promise.all([
          api.request('run', api.runPath(selectedId!)), api.list('attempt', `${api.runPath(selectedId!)}/attempts`), api.list('event', `${api.runPath(selectedId!)}/events?after=${after}`)
        ]);
        if (stopped || selected.current !== selectedId) return;
        if (nextRun.id !== selectedId || nextAttempts.some(attempt => attempt.run_id !== selectedId) || nextEvents.some(event => event.run_id !== selectedId)) {
          throw new api.WorkflowError('response-invalid', 'The response belongs to another run.', 'Reload the selected run.');
        }
        setRun(nextRun); setAttempts(nextAttempts); setError(current => current?.source === 'detail' ? null : current);
        setEvents(current => [...new Map([...current, ...nextEvents].map(event => [event.sequence, event])).values()].sort((a, b) => a.sequence - b.sequence).slice(-50));
        cursor.current = { id: selectedId, sequence: Math.max(after, ...nextEvents.map(event => event.sequence)) };
      } catch (cause) { if (!stopped && selected.current === selectedId) report(cause, 'detail'); }
      finally { if (!stopped) timer = setTimeout(() => void poll(), 1500); }
    }
    void poll();
    return () => { stopped = true; clearTimeout(timer); };
  }, [selectedId, demoSnapshot, report]);
  return { ...snapshot, selectedId, selectRun: setSelectedId, run, attempts, events, loading, authenticationRequired, error: error?.cause, report, clearError: () => setError(null), refresh, updated };
}

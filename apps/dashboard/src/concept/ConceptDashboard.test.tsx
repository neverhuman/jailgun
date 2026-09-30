import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { App } from '../App';
import { workflowFixtures as fixtures } from '../generated/workflow-fixtures';
import { Accounts } from './Accounts';
import { RunProgress } from './RunProgress';
import { Markdown } from './Markdown';
import { artifactText, validate } from './api';

function json(value: unknown, status = 200) { return new Response(JSON.stringify(value), { status, headers: { 'content-type': 'application/json' } }); }
const run = validate('run', fixtures['run-completed']);
const partial = validate('run', fixtures['run-partial']);
let submissions: Record<string, unknown>[];
let authenticated: boolean;
let rejectSubmission: boolean;
let responseRun: typeof run | null;
let fetcher: ReturnType<typeof vi.fn>;
beforeEach(() => {
  submissions = []; authenticated = true; rejectSubmission = false; responseRun = null;
  window.history.replaceState(null, '', '/#new');
  fetcher = vi.fn(async (path: string, init?: RequestInit) => {
    if (path === '/api/session/pair') { authenticated = true; return new Response(null, { status: 204 }); }
    if (!authenticated) return json({ code: 'authentication-required', message: 'Pair this browser.', next_action: 'Run jailgun setup.' }, 401);
    if (path === '/api/accounts') return json(fixtures.accounts);
    if (path === '/api/accounts/sessions') return json([fixtures['account-ready']]);
    if (path === '/api/concept-runs') {
      submissions.push(JSON.parse(String(init?.body)));
      if (rejectSubmission) throw new Error('connection lost after send');
      responseRun = run; return json({ run_id: run.id, status: 'queued', run_url: `/api/runs/${run.id}`, result_url: `/api/runs/${run.id}/result` }, 202);
    }
    if (path === '/api/runs?kind=concept') return json(responseRun ? [responseRun] : []);
    if (path.endsWith('/attempts')) return json(fixtures['completed-attempts']);
    if (path.includes('/events?')) return json(fixtures.events);
    if (path === `/api/runs/${run.id}`) return json(responseRun);
    throw new Error(`Unexpected test request: ${path}`);
  });
  vi.stubGlobal('fetch', fetcher);
});
afterEach(() => { vi.unstubAllGlobals(); window.history.replaceState(null, '', '/'); });
async function draft() {
  fireEvent.change(await screen.findByLabelText('What do you want to explore?'), { target: { value: 'A community tool library' } });
  fireEvent.change(screen.getByLabelText('ChatGPT account'), { target: { value: 'demo-account' } });
}
async function navigate(hash: string) {
  await act(async () => { window.history.replaceState(null, '', `/#${hash}`); fireEvent(window, new Event('hashchange')); });
}
it('uses the canonical criteria and one idempotency key when retrying an uncertain submission', async () => {
  render(<App />); await draft(); rejectSubmission = true;
  fireEvent.click(screen.getByRole('button', { name: 'Explore this concept' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('could not be reached');
  expect(submissions).toHaveLength(1);
  rejectSubmission = false;
  fireEvent.click(screen.getByRole('button', { name: 'Explore this concept' }));
  await screen.findByRole('heading', { name: 'Run progress' });
  expect(submissions).toHaveLength(2);
  expect(submissions[1].idempotency_key).toBe(submissions[0].idempotency_key);
  expect(submissions[0].criteria).toEqual(run.request.criteria);
  await navigate('new');
  expect(screen.getByLabelText('What do you want to explore?')).toHaveValue('A community tool library');
  fireEvent.click(screen.getByRole('button', { name: 'Explore this concept' }));
  await waitFor(() => expect(submissions).toHaveLength(3));
  expect(submissions[2].idempotency_key).not.toBe(submissions[0].idempotency_key);
});
it('preserves draft input and the selected run through operator session expiry and pairing', async () => {
  responseRun = run; render(<App />); await draft();
  await navigate('runs'); await screen.findByRole('heading', { name: 'Run progress' });
  await navigate('new'); authenticated = false;
  fireEvent.click(screen.getByRole('button', { name: 'Explore this concept' }));
  fireEvent.change(await screen.findByLabelText('Pairing code'), { target: { value: 'one-use-code' } });
  fireEvent.click(screen.getByRole('button', { name: 'Pair browser' }));
  await screen.findByLabelText('What do you want to explore?');
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  expect(screen.getByLabelText('What do you want to explore?')).toHaveValue('A community tool library');
  expect(screen.getByLabelText('ChatGPT account')).toHaveValue('demo-account');
  await navigate('runs');
  expect(screen.getByLabelText('Selected run')).toHaveValue(run.id);
  expect(window.localStorage.length).toBe(0);
});
it('rejects duplicate criteria and never falls back to synthetic data on API failure', async () => {
  render(<App />); await draft();
  fireEvent.change(screen.getByLabelText('Criterion 2'), { target: { value: 'Usefulness' } });
  fireEvent.click(screen.getByRole('button', { name: 'Explore this concept' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('unique criteria');
  expect(submissions).toHaveLength(0);
});
it('shows an actionable service error with no synthetic accounts', async () => {
  fetcher.mockRejectedValue(new Error('offline')); render(<App />);
  expect(await screen.findByRole('alert')).toHaveTextContent('could not be reached');
  expect(screen.queryByText(/demo-account/)).not.toBeInTheDocument();
  expect(screen.queryByText(/Demo mode/)).not.toBeInTheDocument();
  expect(screen.getByRole('heading', { name: 'Unable to load your installation' })).toBeInTheDocument();
  expect(screen.queryByRole('heading', { name: 'New concept' })).not.toBeInTheDocument();
  fetcher.mockImplementation(async () => json([]));
  fireEvent.click(screen.getByRole('button', { name: 'Reconnect to service' }));
  await screen.findByRole('heading', { name: 'New concept' });
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});
it('offers explicit subset continuation only with enough complete candidates and no active attempts', () => {
  const action = vi.fn(async () => true);
  const attempts = fixtures['partial-attempts'].map(value => validate('attempt', value));
  const view = render(<RunProgress run={partial} attempts={attempts} events={[]} busy={false} act={action} showResults={() => {}} />);
  fireEvent.click(screen.getByRole('button', { name: 'Continue with 3 complete candidates' }));
  expect(action).toHaveBeenCalledWith(`/api/runs/${partial.id}/resume`, { allow_incomplete: true });
  view.rerender(<RunProgress run={partial} attempts={[{ ...attempts[0], state: 'uncertain' }]} events={[]} busy={false} act={action} showResults={() => {}} />);
  expect(screen.queryByRole('button', { name: /Continue with/ })).not.toBeInTheDocument();
});
it('requires confirmation of the observed identity and sends the selected model', async () => {
  const action = vi.fn(async () => true);
  render(<Accounts sessions={[validate('account_session', fixtures['account-verifying'])]} readiness={[]} busy={false} act={action} demo={false} />);
  expect(screen.getByText('synthetic-demo-identity', { exact: false })).toBeInTheDocument();
  fireEvent.change(screen.getByLabelText('Model for new runs'), { target: { value: 'Synthetic reasoning' } });
  fireEvent.click(screen.getByRole('button', { name: 'Confirm account and model' }));
  expect(action).toHaveBeenCalledWith('/api/accounts/demo-account/confirm', { identity: fixtures['account-verifying'].observation!.identity, model: { mode: 'specific', name: 'Synthetic reasoning' } });
});
it('renders safe Markdown while preserving lists, code and links', () => {
  const view = render(<Markdown text={'# Proposal\n\n- First\n- Second\n\n```js\nconst x = 1;\n```\n\n[Evidence](https://example.invalid)\n\n[Bad](javascript:alert(1))\n\n![Remote](https://example.invalid/tracking.png)\n\n<script>alert(1)</script><img src=x onerror=alert(1) />'} />);
  expect(screen.getByRole('heading', { name: 'Proposal' })).toBeInTheDocument();
  expect(screen.getAllByRole('listitem')).toHaveLength(2);
  expect(view.container.querySelector('pre code')).toHaveTextContent('const x = 1;');
  expect(screen.getByRole('link', { name: 'Evidence' })).toHaveAttribute('rel', 'noopener noreferrer');
  expect(view.container.querySelector('script, img, [onerror], a[href^="javascript:"]')).toBeNull();
});
it('verifies artifact length and hash before exposing its contents', async () => {
  const artifact = run.artifacts.find(a => a.name === 'final.md')!;
  fetcher.mockResolvedValue(new Response('tampered output'));
  await expect(artifactText(run.id, artifact)).rejects.toMatchObject({ code: 'artifact-integrity' });
  await expect(artifactText('other-run', artifact)).rejects.toMatchObject({ code: 'artifact-ownership' });
});
it('renders a clearly identified demo without any network requests', async () => {
  window.history.replaceState(null, '', '/#results'); render(<App mode="fixture" />);
  expect(await screen.findByRole('heading', { name: 'Your final concept' })).toBeInTheDocument();
  expect(screen.getByText(/Demo mode — synthetic data/)).toBeInTheDocument();
  expect(await within(screen.getByRole('article', { name: 'final.md' })).findByRole('heading', { name: 'A small tool library, tested first' })).toBeInTheDocument();
  expect(fetcher).not.toHaveBeenCalled();
});

it('keeps the active screen when the keyboard skip link changes the fragment', async () => {
  render(<App />); await draft();
  await navigate('workspace');
  expect(screen.getByRole('heading', { name: 'New concept' })).toBeInTheDocument();
});

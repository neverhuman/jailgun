import { render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { App } from '../App';

afterEach(() => { vi.unstubAllGlobals(); window.history.replaceState(null, '', '/'); });
it.each(['/', '/#new', '/#runs', '/#results', '/?advanced=1'])('denies an unauthorized browser at %s without exposing account or run data', async path => {
  window.history.replaceState(null, '', path);
  const fetcher = vi.fn(async (_url: string, options?: RequestInit) => {
    expect(options?.credentials ?? 'same-origin').toBe('same-origin');
    expect(new Headers(options?.headers).has('authorization')).toBe(false);
    return new Response(JSON.stringify({ code: 'unauthorized', message: 'Pair the operator browser.', next_action: 'Run jailgun setup.' }), { status: 401 });
  });
  vi.stubGlobal('fetch', fetcher);
  render(<App />);
  expect(await screen.findByRole('heading', { name: 'Pair this browser' })).toBeInTheDocument();
  expect(screen.queryByRole('heading', { name: 'Accounts' })).not.toBeInTheDocument();
  expect(screen.queryByRole('heading', { name: 'Run progress' })).not.toBeInTheDocument();
  expect(screen.queryByRole('link', { name: 'Agent summary' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Connect ChatGPT' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Explore this concept' })).not.toBeInTheDocument();
  expect(screen.queryByText(/fixture-run|demo-account/)).not.toBeInTheDocument();
  expect(window.localStorage.length).toBe(0);
  expect(fetcher).toHaveBeenCalled();
});

it.each(['/#accounts', '/#new'])('keeps forbidden operator data and controls unavailable at %s when an automation credential lacks access', async path => {
  window.history.replaceState(null, '', path);
  vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ code: 'forbidden', message: 'Operator access required.', next_action: 'Pair the operator browser.' }), { status: 403 })));
  render(<App />);
  expect(await screen.findByRole('alert')).toHaveTextContent('Operator access required. Pair the operator browser.');
  expect(screen.queryByText(/fixture-run|demo-account|synthetic-demo-identity/)).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Confirm account and model' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Connect ChatGPT' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Explore this concept' })).not.toBeInTheDocument();
  expect(screen.queryByRole('link', { name: /Download/ })).not.toBeInTheDocument();
});

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { StrictMode } from 'react';
import { ArchiveDashboard as App } from './ArchiveDashboard';
import { jsonResponse, setupDashboardMocks } from './App.testSupport';

setupDashboardMocks();
afterEach(() => window.history.replaceState(null, '', '/'));

it('exchanges a setup link exactly once and removes its credential from browser history', async () => {
  const code = 'bd4d3651-1952-4017-9092-105f27cc6d63';
  window.history.replaceState(null, '', `/#pair=${code}&next=accounts`);
  let exchanges = 0;
  vi.stubGlobal('fetch', vi.fn(async (url: string, options?: RequestInit) => {
    expect(window.location.hash).not.toContain(code);
    if (url === '/api/session/pair') {
      exchanges += 1;
      expect(JSON.parse(String(options?.body))).toEqual({ code });
      return { ok: true, status: 204 };
    }
    return jsonResponse([]);
  }));
  render(<StrictMode><App /></StrictMode>);
  expect(await screen.findByText('No runs yet')).toBeInTheDocument();
  expect(exchanges).toBe(1);
  expect(window.location.hash).toBe('#accounts');
  expect(window.localStorage.length).toBe(0);
});

it('removes an invalid link and permits manual pairing without submitting the bad code', async () => {
  window.history.replaceState(null, '', '/#pair=invalid');
  const fetcher = vi.fn(async (_url: string) => ({ ok: false, status: 401 }));
  vi.stubGlobal('fetch', fetcher);
  render(<App />);
  expect(await screen.findByRole('alert')).toHaveTextContent('pairing link is invalid');
  expect(fetcher.mock.calls.every(args => args[0] !== '/api/session/pair')).toBe(true);
  expect(window.location.hash).toBe('#accounts');
});

it('accepts successive setup links in an already loaded dashboard tab', async () => {
  let exchanges = 0;
  vi.stubGlobal('fetch', vi.fn(async (url: string) => {
    if (url === '/api/session/pair') { exchanges += 1; return { ok: true, status: 204 }; }
    return jsonResponse([]);
  }));
  render(<App />);
  await screen.findByText('No runs yet');
  window.history.replaceState(null, '', '/#pair=invalid');
  fireEvent(window, new Event('hashchange'));
  expect(await screen.findByRole('alert')).toHaveTextContent('pairing link is invalid');
  expect(exchanges).toBe(0);
  window.history.replaceState(null, '', '/#pair=bd4d3651-1952-4017-9092-105f27cc6d63');
  fireEvent(window, new Event('hashchange'));
  await waitFor(() => expect(exchanges).toBe(1));
  expect(await screen.findByText('No runs yet')).toBeInTheDocument();
  expect(window.location.hash).toBe('#accounts');
});

it('pairs without storing a long-lived credential and refreshes API data', async () => {
  let paired = false;
  const fetcher = vi.fn(async (url: string, options?: RequestInit) => {
    if (url === '/api/session/pair') {
      expect(options?.method).toBe('POST');
      expect(JSON.parse(String(options?.body))).toEqual({ code: 'one-use-code' });
      paired = true;
      return { ok: true, status: 204 };
    }
    return paired ? jsonResponse([]) : { ok: false, status: 401 };
  });
  vi.stubGlobal('fetch', fetcher);
  render(<App />);
  const input = await screen.findByLabelText('Pairing code');
  fireEvent.change(input, { target: { value: 'one-use-code' } });
  fireEvent.click(screen.getByRole('button', { name: 'Pair browser' }));
  await waitFor(() => expect(screen.getByText('No runs yet')).toBeInTheDocument());
  expect(window.localStorage.length).toBe(0);
});

it('keeps invalid pairing actionable', async () => {
  vi.stubGlobal('fetch', vi.fn(async () => ({ ok: false, status: 401 })));
  render(<App />);
  const input = await screen.findByLabelText('Pairing code');
  fireEvent.change(input, { target: { value: 'expired-code' } });
  fireEvent.click(screen.getByRole('button', { name: 'Pair browser' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('invalid or expired');
});

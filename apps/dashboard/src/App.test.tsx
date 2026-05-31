import { render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { App } from './App';
import { fixtureRuns } from './fixtures';
import type { JailgunEvent } from './types';

class MockWebSocket {
  static instances: MockWebSocket[] = [];
  static OPEN = 1;
  readyState = MockWebSocket.OPEN;
  onmessage: ((message: { data: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  constructor(public url: string) {
    MockWebSocket.instances.push(this);
  }
  close = vi.fn();
  emit(event: JailgunEvent) {
    this.onmessage?.({ data: JSON.stringify(event) });
  }
}

beforeEach(() => {
  vi.stubGlobal('fetch', vi.fn(async (url: string) => {
    if (url === '/api/runs') {
      return jsonResponse(fixtureRuns);
    }
    if (url.startsWith('/api/receipts/')) {
      return jsonResponse({ run_id: 'fixture-run', receipts: [{ tab_id: 1, sha256: 'abc123' }] });
    }
    return { ok: false, status: 404, json: async () => ({}) };
  }));
  MockWebSocket.instances = [];
  vi.stubGlobal('WebSocket', MockWebSocket);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

it('renders live runs table and tab cards', async () => {
  render(<App />);
  expect(await screen.findByText('fixture-run')).toBeInTheDocument();
  expect(screen.getByRole('table')).toBeInTheDocument();
  expect(screen.getByText('Tab 1')).toBeInTheDocument();
  expect(screen.getByText('remote-job-launched')).toBeInTheDocument();
});

it('renders chart and prompt counters', async () => {
  render(<App />);
  expect(await screen.findByLabelText('download latency chart')).toBeInTheDocument();
  const counters = screen.getByLabelText('GitHub prompt counters');
  expect(counters).toHaveTextContent('Denied');
  expect(counters).toHaveTextContent('Info Allowed');
});

it('uses fixture fallback for empty/error states when fetch fails', async () => {
  vi.stubGlobal('fetch', vi.fn(async () => {
    throw new Error('network down');
  }));
  render(<App />);
  expect((await screen.findAllByText('fixture-run')).length).toBeGreaterThan(0);
});

it('applies WebSocket event updates', async () => {
  render(<App />);
  await screen.findAllByText('fixture-run');
  const socket = MockWebSocket.instances[0];
  socket.emit({
    run_id: 'fixture-run',
    tab_id: 4,
    timestamp: '2026-01-01T00:00:10Z',
    kind: 'remote-safety',
    severity: 'warn',
    message: 'preserved divergent head',
    fields: { policy: 'preserve-reset' }
  });
  await waitFor(() => expect(screen.getByText('preserved divergent head')).toBeInTheDocument());
});

function jsonResponse(body: unknown) {
  return {
    ok: true,
    status: 200,
    json: async () => body
  };
}

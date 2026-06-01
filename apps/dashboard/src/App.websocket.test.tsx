import { render, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';

import { App } from './App';
import { MockWebSocket, jsonResponse, setupDashboardMocks } from './App.testSupport';

setupDashboardMocks();

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

it('creates a visible run from WebSocket events when the API starts empty', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string) => {
      if (url === '/api/runs') {
        return jsonResponse([]);
      }
      if (url.startsWith('/api/receipts/')) {
        return jsonResponse({ run_id: 'live-run', receipts: [] });
      }
      return { ok: false, status: 404, json: async () => ({}) };
    })
  );

  render(<App />);
  expect(await screen.findByText('No runs yet')).toBeInTheDocument();

  MockWebSocket.instances[0].emit({
    run_id: 'live-run',
    tab_id: 1,
    timestamp: '2026-01-01T00:00:10Z',
    kind: 'download-receipt',
    severity: 'info',
    message: 'download complete',
    fields: { sha256: 'abcdef0123456789' }
  });

  await waitFor(() => expect(screen.getAllByText('live-run').length).toBeGreaterThan(0));
  expect(screen.getByText('Tab 1')).toBeInTheDocument();
  expect(screen.getByText('download complete')).toBeInTheDocument();
});

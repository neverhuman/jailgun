import { render, screen, waitFor } from '@testing-library/react';
import { expect, it } from 'vitest';

import { App } from './App';
import { MockWebSocket, setupDashboardMocks } from './App.testSupport';

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

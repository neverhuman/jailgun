import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { expect, it } from 'vitest';

import { App } from './App';
import { MockWebSocket, setupDashboardMocks } from './App.testSupport';

setupDashboardMocks();

it('expands the drilldown when the row toggle is clicked', async () => {
  render(<App />);
  await screen.findByLabelText('tab 1 row');
  fireEvent.click(screen.getByLabelText('expand tab 1'));
  await screen.findByLabelText('tab 1 detail');
  expect(screen.getByText(/Local sha256/i)).toBeInTheDocument();
});

it('shows a Closed pill when a tab-closed event lands', async () => {
  render(<App />);
  await screen.findByLabelText('tab 1 row');
  MockWebSocket.instances[0].emit({
    run_id: 'fixture-run',
    tab_id: 1,
    timestamp: '2026-01-01T00:00:09Z',
    kind: 'tab-closed',
    severity: 'info',
    message: 'tab closed',
    fields: { tab_status: 'closed' }
  });
  await waitFor(() => expect(screen.getByLabelText('tab closed')).toBeInTheDocument());
});

it('renders the failure trace tooltip on hover when outcome failed', async () => {
  render(<App />);
  await screen.findByLabelText('tab 1 row');
  MockWebSocket.instances[0].emit({
    run_id: 'fixture-run',
    tab_id: 1,
    timestamp: '2026-01-01T00:00:11Z',
    kind: 'deploy-finished',
    severity: 'error',
    message: 'deploy failed hard',
    fields: {
      outcome: 'failed-hard',
      exit_code: '127',
      remote_command: 'bash ci-fast-push.sh',
      log_tail: 'bash: ci-fast-push.sh: No such file or directory'
    }
  });
  const failureButton = await screen.findByLabelText('show failure trace');
  fireEvent.mouseEnter(failureButton);
  const tooltip = await screen.findByLabelText('failure trace');
  expect(tooltip).toHaveTextContent('outcome=failed-hard');
  expect(tooltip).toHaveTextContent('No such file or directory');
});

it('lists files changed inside the drilldown', async () => {
  render(<App />);
  await screen.findByLabelText('tab 1 row');
  MockWebSocket.instances[0].emit({
    run_id: 'fixture-run',
    tab_id: 1,
    timestamp: '2026-01-01T00:00:12Z',
    kind: 'deploy-finished',
    severity: 'info',
    message: 'deploy ok',
    fields: {
      outcome: 'succeeded',
      files_changed: 'crates/foo/src/lib.rs\ncrates/foo/Cargo.toml'
    }
  });
  fireEvent.click(screen.getByLabelText('expand tab 1'));
  await screen.findByLabelText('tab 1 detail');
  expect(screen.getByText('crates/foo/src/lib.rs')).toBeInTheDocument();
  expect(screen.getByText('crates/foo/Cargo.toml')).toBeInTheDocument();
});

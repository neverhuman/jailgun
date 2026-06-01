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
      changed_paths: 'crates/foo/src/lib.rs\ncrates/foo/Cargo.toml'
    }
  });
  fireEvent.click(screen.getByLabelText('expand tab 1'));
  await screen.findByLabelText('tab 1 detail');
  expect(screen.getByText('crates/foo/src/lib.rs')).toBeInTheDocument();
  expect(screen.getByText('crates/foo/Cargo.toml')).toBeInTheDocument();
});

it('shows a FINISHED pill with short post_head when deploy succeeds', async () => {
  render(<App />);
  await screen.findByLabelText('tab 1 row');
  MockWebSocket.instances[0].emit({
    run_id: 'fixture-run',
    tab_id: 1,
    timestamp: '2026-01-01T00:00:13Z',
    kind: 'deploy-finished',
    severity: 'info',
    message: 'deploy ok',
    fields: {
      outcome: 'succeeded',
      post_head: 'df9437530a1110e1a784a53fa7feaefca43383ab',
      deploy_status: 'succeeded'
    }
  });
  await waitFor(() => {
    const pills = screen.getAllByLabelText('tab outcome state');
    expect(pills.some((pill) => pill.textContent?.includes('FINISHED'))).toBe(true);
  });
  const pills = screen.getAllByLabelText('tab outcome state');
  const finished = pills.find((pill) => pill.textContent?.includes('FINISHED'));
  expect(finished?.textContent).toContain('df94375');
});

it('shows a PRESERVED pill when deploy outcome is failed-preserved', async () => {
  render(<App />);
  await screen.findByLabelText('tab 2 row');
  MockWebSocket.instances[0].emit({
    run_id: 'fixture-run',
    tab_id: 2,
    timestamp: '2026-01-01T00:00:14Z',
    kind: 'deploy-finished',
    severity: 'warn',
    message: 'preserved divergent head',
    fields: {
      outcome: 'failed-preserved',
      deploy_status: 'failed-preserved',
      preserved_ref: 'preserve/tab-2-2026'
    }
  });
  await waitFor(() => {
    const pills = screen.getAllByLabelText('tab outcome state');
    expect(pills.some((pill) => pill.textContent === 'PRESERVED')).toBe(true);
  });
});

it('renders a Pushed metric in the run header when post_head is set', async () => {
  render(<App />);
  await screen.findByLabelText('run progress metrics');
  MockWebSocket.instances[0].emit({
    run_id: 'fixture-run',
    tab_id: 1,
    timestamp: '2026-01-01T00:00:15Z',
    kind: 'deploy-finished',
    severity: 'info',
    message: 'deploy ok',
    fields: {
      outcome: 'succeeded',
      post_head: 'aaaaaa'
    }
  });
  await waitFor(() => expect(screen.getByText('Pushed')).toBeInTheDocument());
});

it('surfaces post commit in the drilldown when post_head is set', async () => {
  render(<App />);
  await screen.findByLabelText('tab 1 row');
  MockWebSocket.instances[0].emit({
    run_id: 'fixture-run',
    tab_id: 1,
    timestamp: '2026-01-01T00:00:16Z',
    kind: 'deploy-finished',
    severity: 'info',
    message: 'deploy ok',
    fields: {
      outcome: 'succeeded',
      post_head: 'df9437530a1110e1a784a53fa7feaefca43383ab'
    }
  });
  fireEvent.click(screen.getByLabelText('expand tab 1'));
  await screen.findByLabelText('tab 1 detail');
  expect(screen.getByText(/Post commit/i)).toBeInTheDocument();
  expect(screen.getByText('df9437530a11')).toBeInTheDocument();
});

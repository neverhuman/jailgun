import { render, screen } from '@testing-library/react';
import { expect, it } from 'vitest';

import { App } from './App';
import { setupDashboardMocks } from './App.testSupport';

setupDashboardMocks();

it('renders the run header and per-tab rows', async () => {
  render(<App />);
  expect(await screen.findByText('fixture-run', { exact: false })).toBeInTheDocument();
  expect(screen.getByLabelText('run progress metrics')).toBeInTheDocument();
  expect(screen.getByLabelText('tab 1 row')).toBeInTheDocument();
  expect(screen.getByLabelText('tab 2 row')).toBeInTheDocument();
  expect(screen.getByLabelText('tab 3 row')).toBeInTheDocument();
});

it('renders a 5-segment progress bar for every tab', async () => {
  render(<App />);
  await screen.findByLabelText('tab 1 row');
  expect(screen.getByLabelText('tab 1 progress')).toBeInTheDocument();
  expect(screen.getByLabelText('tab 2 progress')).toBeInTheDocument();
  expect(screen.getByLabelText('tab 3 progress')).toBeInTheDocument();
});

it('renders elapsed time in the run header', async () => {
  render(<App />);
  expect(await screen.findByLabelText('elapsed time')).toBeInTheDocument();
});

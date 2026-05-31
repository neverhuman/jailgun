import { render, screen } from '@testing-library/react';
import { expect, it } from 'vitest';

import { App } from './App';
import { setupDashboardMocks } from './App.testSupport';

setupDashboardMocks();

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

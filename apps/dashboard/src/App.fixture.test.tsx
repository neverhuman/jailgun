import { render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';

import { ArchiveDashboard as App } from './ArchiveDashboard';
import { setupDashboardMocks } from './App.testSupport';

setupDashboardMocks();

it('shows API errors without substituting fixture data', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => {
      throw new Error('network down');
    })
  );
  render(<App />);
  expect(await screen.findByText('network down')).toBeInTheDocument();
  expect(screen.queryByText(/fixture-run/)).not.toBeInTheDocument();
});

it('uses synthetic data only when demo mode is explicitly selected', async () => {
  const fetcher = vi.fn();
  vi.stubGlobal('fetch', fetcher);
  render(<App mode="fixture" />);
  expect(await screen.findByText('Demo mode — synthetic data')).toBeInTheDocument();
  expect((await screen.findAllByText(/fixture-run/)).length).toBeGreaterThan(0);
  expect(fetcher).not.toHaveBeenCalled();
});

import { render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';

import { App } from './App';
import { setupDashboardMocks } from './App.testSupport';

setupDashboardMocks();

it('uses explicit fixture mode for empty/error states when fetch fails', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => {
      throw new Error('network down');
    })
  );
  render(<App />);
  expect((await screen.findAllByText('fixture-run')).length).toBeGreaterThan(0);
});

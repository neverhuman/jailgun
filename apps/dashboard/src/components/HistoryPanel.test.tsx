import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { fixtureHistory } from '../fixtures';
import { HistoryPanel } from './HistoryPanel';

function jsonResponse(body: unknown) {
  return { ok: true, status: 200, json: async () => body };
}

beforeEach(() => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string) => {
      if (url === '/api/history') {
        return { ok: false, status: 404, json: async () => ({}) };
      }
      return { ok: false, status: 404, json: async () => ({}) };
    })
  );
});

afterEach(() => {
  vi.unstubAllGlobals();
});

it('renders empty state when no history is returned', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => jsonResponse([]))
  );
  render(<HistoryPanel hasActiveRun={false} />);
  expect(await screen.findByText('No history available.')).toBeInTheDocument();
});

it('renders summary stats with fixture data', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => jsonResponse(fixtureHistory))
  );
  render(<HistoryPanel hasActiveRun={false} />);
  expect(await screen.findByText('Total Runs')).toBeInTheDocument();
  expect(screen.getByText('4')).toBeInTheDocument();
  expect(screen.getByText('Avg Success Rate')).toBeInTheDocument();
  expect(screen.getByText('Total Deploys')).toBeInTheDocument();
  expect(screen.getByText('11')).toBeInTheDocument();
});

it('toggle expands and collapses panel', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => jsonResponse(fixtureHistory))
  );
  render(<HistoryPanel hasActiveRun={true} />);

  // Should start collapsed when there's an active run
  const toggle = screen.getByRole('button', { name: /run history/i });
  expect(toggle).toHaveAttribute('aria-expanded', 'false');

  // Expand
  fireEvent.click(toggle);
  expect(toggle).toHaveAttribute('aria-expanded', 'true');
  expect(await screen.findByText('Total Runs')).toBeInTheDocument();

  // Collapse again
  fireEvent.click(toggle);
  expect(toggle).toHaveAttribute('aria-expanded', 'false');
  expect(screen.queryByText('Total Runs')).not.toBeInTheDocument();
});

it('has correct aria-label on section', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => jsonResponse(fixtureHistory))
  );
  render(<HistoryPanel hasActiveRun={false} />);
  expect(screen.getByLabelText('history panel')).toBeInTheDocument();
});

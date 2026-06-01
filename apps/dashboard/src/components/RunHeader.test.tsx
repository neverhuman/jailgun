import { render, screen } from '@testing-library/react';
import { expect, it } from 'vitest';

import { RunHeader } from './RunHeader';
import type { RunSnapshot } from '../types';

function buildRun(
  tabCount: number,
  overrides: Partial<RunSnapshot> = {}
): RunSnapshot {
  return {
    run_id: 'loop-run',
    started_at: '2026-01-01T00:00:00Z',
    finished_at: null,
    status: 'running',
    batch_tabs: 7,
    loop_count: 1,
    planned_tabs: 14,
    deploy_queue: 'running',
    denied_github_prompts: 0,
    allowed_info_prompts: 0,
    early_stops_succeeded: 0,
    early_stops_attempted: 0,
    tabs: Array.from({ length: tabCount }, (_, index) => ({
      tab_id: index + 1,
      status: 'downloaded',
      page_url: `https://chatgpt.com/c/${index + 1}`,
      archive_sha256: `sha-${index + 1}`,
      download_latency_ms: 1200,
      deploy_status: 'validated',
      prompt_policy_decision: null,
      early_stop_outcome: null
    })),
    ...overrides
  };
}

it('shows the loop countdown and decrements it when the next batch begins', () => {
  const { rerender } = render(
    <RunHeader run={buildRun(7)} connection="open" dataSource="api" events={[]} />
  );

  expect(screen.getByLabelText('looping status')).toHaveTextContent('1 left');
  expect(screen.getByLabelText('run progress metrics')).toHaveTextContent('7/14');

  rerender(<RunHeader run={buildRun(8)} connection="open" dataSource="api" events={[]} />);

  expect(screen.getByLabelText('looping status')).toHaveTextContent('0 left');
  expect(screen.getByLabelText('run progress metrics')).toHaveTextContent('8/14');
});

it('shows the early stops metric with neutral tone when nothing attempted', () => {
  render(<RunHeader run={buildRun(0)} connection="open" dataSource="api" events={[]} />);
  const metrics = screen.getByLabelText('run progress metrics');
  expect(metrics).toHaveTextContent('Early stops');
  expect(metrics).toHaveTextContent('0/0');
  const earlyStopPill = Array.from(metrics.querySelectorAll('.runMetric')).find((node) =>
    node.textContent?.includes('Early stops')
  );
  expect(earlyStopPill?.className).toContain('neutral');
});

it('renders the early stops metric with warn tone on partial success', () => {
  render(
    <RunHeader
      run={buildRun(7, { early_stops_succeeded: 3, early_stops_attempted: 5 })}
      connection="open"
      dataSource="api"
      events={[]}
    />
  );
  const metrics = screen.getByLabelText('run progress metrics');
  expect(metrics).toHaveTextContent('3/5');
  const pill = metrics.querySelector('.runMetric.warn');
  expect(pill?.textContent ?? '').toContain('Early stops');
});

it('renders the early stops metric with danger tone when nothing succeeded', () => {
  render(
    <RunHeader
      run={buildRun(7, { early_stops_succeeded: 0, early_stops_attempted: 7 })}
      connection="open"
      dataSource="api"
      events={[]}
    />
  );
  expect(screen.getByLabelText('run progress metrics').querySelector('.runMetric.danger')).not.toBeNull();
});

it('renders the early stops metric with ok tone when fully succeeded', () => {
  render(
    <RunHeader
      run={buildRun(7, { early_stops_succeeded: 7, early_stops_attempted: 7 })}
      connection="open"
      dataSource="api"
      events={[]}
    />
  );
  const okPills = screen.getByLabelText('run progress metrics').querySelectorAll('.runMetric.ok');
  const labels = Array.from(okPills).map((node) => node.textContent ?? '');
  expect(labels.some((text) => text.includes('Early stops'))).toBe(true);
});

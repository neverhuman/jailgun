import { render, screen } from '@testing-library/react';
import { expect, it } from 'vitest';

import { RunHeader } from './RunHeader';
import type { RunSnapshot } from '../types';

function buildRun(tabCount: number): RunSnapshot {
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
    tabs: Array.from({ length: tabCount }, (_, index) => ({
      tab_id: index + 1,
      status: 'downloaded',
      page_url: `https://chatgpt.com/c/${index + 1}`,
      archive_sha256: `sha-${index + 1}`,
      download_latency_ms: 1200,
      deploy_status: 'validated',
      prompt_policy_decision: null
    }))
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

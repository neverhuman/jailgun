import type { JailgunEvent, ReceiptResponse, RunSnapshot } from './types';

export const fixtureRuns: RunSnapshot[] = [
  {
    run_id: 'fixture-run',
    started_at: '2026-01-01T00:00:00Z',
    finished_at: null,
    status: 'running',
    batch_tabs: 3,
    loop_count: 0,
    planned_tabs: 3,
    deploy_queue: 'running',
    denied_github_prompts: 2,
    allowed_info_prompts: 1,
    tabs: [
      {
        tab_id: 1,
        status: 'downloaded',
        page_url: 'https://chatgpt.com/c/example-one',
        archive_sha256: 'abc123',
        download_latency_ms: 1200,
        deploy_status: 'validated',
        prompt_policy_decision: 'deny'
      },
      {
        tab_id: 2,
        status: 'remote-running',
        page_url: 'https://chatgpt.com/c/example-two',
        archive_sha256: 'def456',
        download_latency_ms: 1700,
        deploy_status: 'remote-job-launched',
        prompt_policy_decision: 'allow-info'
      },
      {
        tab_id: 3,
        status: 'waiting-for-tar',
        page_url: 'https://chatgpt.com/c/example-three',
        archive_sha256: null,
        download_latency_ms: null,
        deploy_status: 'waiting-for-tar',
        prompt_policy_decision: null
      }
    ]
  }
];

export const fixtureEvents: JailgunEvent[] = [
  {
    run_id: 'fixture-run',
    tab_id: null,
    timestamp: '2026-01-01T00:00:00Z',
    kind: 'run-started',
    severity: 'info',
    message: 'fixture run started',
    fields: {}
  },
  {
    run_id: 'fixture-run',
    tab_id: 1,
    timestamp: '2026-01-01T00:00:03Z',
    kind: 'download-receipt',
    severity: 'info',
    message: 'archive receipt confirmed',
    fields: { sha256: 'abc123' }
  },
  {
    run_id: 'fixture-run',
    tab_id: 2,
    timestamp: '2026-01-01T00:00:05Z',
    kind: 'remote-safety',
    severity: 'warn',
    message: 'preserve-reset ready',
    fields: { policy: 'preserve-reset' }
  }
];

export const fixtureReceipts: ReceiptResponse = {
  run_id: 'fixture-run',
  receipts: [
    {
      tab_id: 1,
      sha256: 'abc123',
      artifact_path: 'receipts/fixture-run/tab-01-source.tar.gz',
      recorded_at: '2026-01-01T00:00:03Z'
    },
    {
      tab_id: 2,
      sha256: 'def456',
      artifact_path: 'receipts/fixture-run/tab-02-source.tar.gz',
      recorded_at: '2026-01-01T00:00:05Z'
    }
  ]
};

import type { JailgunEvent, ReceiptResponse, RunHistoryEntry, RunSnapshot } from './types';

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
    early_stops_succeeded: 0,
    early_stops_attempted: 0,
    tabs: [
      {
        tab_id: 1,
        status: 'downloaded',
        page_url: 'https://chatgpt.com/c/example-one',
        archive_sha256: 'abc123',
        download_latency_ms: 1200,
        deploy_status: 'validated',
        prompt_policy_decision: 'deny',
        early_stop_outcome: null,
        browser_profile: 'writer',
        browser_profile_dir: null,
        browser_slot: 1,
        cdp_url: null
      },
      {
        tab_id: 2,
        status: 'remote-running',
        page_url: 'https://chatgpt.com/c/example-two',
        archive_sha256: 'def456',
        download_latency_ms: 1700,
        deploy_status: 'remote-job-launched',
        prompt_policy_decision: 'allow-info',
        early_stop_outcome: null,
        browser_profile: 'reviewer',
        browser_profile_dir: null,
        browser_slot: 2,
        cdp_url: null
      },
      {
        tab_id: 3,
        status: 'waiting-for-tar',
        page_url: 'https://chatgpt.com/c/example-three',
        archive_sha256: null,
        download_latency_ms: null,
        deploy_status: 'waiting-for-tar',
        prompt_policy_decision: null,
        early_stop_outcome: null,
        browser_profile: 'writer',
        browser_profile_dir: null,
        browser_slot: 1,
        cdp_url: null
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

export const fixtureHistory: RunHistoryEntry[] = [
  {
    run_id: 'hist-001',
    started_at: '2026-05-28T10:00:00Z',
    finished_at: '2026-05-28T10:45:00Z',
    status: 'finished',
    batch_tabs: 5,
    loop_count: 0,
    planned_tabs: 5,
    total_tabs: 5,
    tabs_passed: 4,
    tabs_failed: 1,
    tabs_pushed: 3,
    deploy_queue_final: 'done',
    denied_github_prompts: 1,
    allowed_info_prompts: 2,
    early_stops_succeeded: 1,
    early_stops_attempted: 1,
    code_stats: { total_files_changed: 12, total_additions: 340, total_deletions: 90, total_test_count: 8 }
  },
  {
    run_id: 'hist-002',
    started_at: '2026-05-29T14:00:00Z',
    finished_at: '2026-05-29T14:30:00Z',
    status: 'finished',
    batch_tabs: 3,
    loop_count: 1,
    planned_tabs: 6,
    total_tabs: 6,
    tabs_passed: 6,
    tabs_failed: 0,
    tabs_pushed: 5,
    deploy_queue_final: 'done',
    denied_github_prompts: 0,
    allowed_info_prompts: 3,
    early_stops_succeeded: 2,
    early_stops_attempted: 2,
    code_stats: { total_files_changed: 8, total_additions: 210, total_deletions: 45, total_test_count: 14 }
  },
  {
    run_id: 'hist-003',
    started_at: '2026-05-30T09:00:00Z',
    finished_at: '2026-05-30T09:50:00Z',
    status: 'finished',
    batch_tabs: 4,
    loop_count: 0,
    planned_tabs: 4,
    total_tabs: 4,
    tabs_passed: 2,
    tabs_failed: 2,
    tabs_pushed: 1,
    deploy_queue_final: 'done',
    denied_github_prompts: 3,
    allowed_info_prompts: 0,
    early_stops_succeeded: 0,
    early_stops_attempted: 1,
    code_stats: { total_files_changed: 5, total_additions: 120, total_deletions: 60, total_test_count: 4 }
  },
  {
    run_id: 'hist-004',
    started_at: '2026-05-30T16:00:00Z',
    finished_at: '2026-05-30T16:20:00Z',
    status: 'finished',
    batch_tabs: 2,
    loop_count: 0,
    planned_tabs: 2,
    total_tabs: 2,
    tabs_passed: 2,
    tabs_failed: 0,
    tabs_pushed: 2,
    deploy_queue_final: 'done',
    denied_github_prompts: 0,
    allowed_info_prompts: 1,
    early_stops_succeeded: 1,
    early_stops_attempted: 1,
    code_stats: null
  }
];

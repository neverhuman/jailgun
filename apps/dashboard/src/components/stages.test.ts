import { describe, expect, it } from 'vitest';

import type { RunSnapshot, TabSnapshot } from '../types';
import {
  deriveStages,
  isTabClosed,
  isTabFailed,
  isTabPassed,
  summarizeOutcome,
  summarizeRunQuality
} from './stages';

function makeTab(overrides: Partial<TabSnapshot> = {}): TabSnapshot {
  return {
    tab_id: 1,
    status: 'opening',
    page_url: 'https://chatgpt.com/c/x',
    archive_sha256: null,
    download_latency_ms: null,
    deploy_status: 'pending',
    prompt_policy_decision: null,
    browser_profile: null,
    browser_profile_dir: null,
    browser_slot: null,
    cdp_url: null,
    ...overrides,
    early_stop_outcome: overrides.early_stop_outcome ?? null
  };
}

function makeRun(overrides: Partial<RunSnapshot> = {}): RunSnapshot {
  return {
    run_id: 'run-1',
    started_at: '2026-01-01T00:00:00Z',
    finished_at: null,
    status: 'running',
    batch_tabs: 3,
    loop_count: 0,
    planned_tabs: 3,
    deploy_queue: 'running',
    denied_github_prompts: 0,
    allowed_info_prompts: 0,
    early_stops_succeeded: 0,
    early_stops_attempted: 0,
    tabs: [makeTab({ tab_id: 1, deploy_status: 'validated', archive_sha256: 'abc' }), makeTab({ tab_id: 2, deploy_status: 'running' }), makeTab({ tab_id: 3, deploy_status: 'running' })],
    ...overrides
  };
}

describe('deriveStages', () => {
  it('starts with polling active and everything else pending', () => {
    const stages = deriveStages(makeTab());
    expect(stages).toHaveLength(5);
    expect(stages[0]).toMatchObject({ key: 'polling', status: 'active' });
    expect(stages[1]).toMatchObject({ key: 'tar', status: 'pending' });
    expect(stages[2]).toMatchObject({ key: 'upload', status: 'pending' });
    expect(stages[3]).toMatchObject({ key: 'ci', status: 'pending' });
    expect(stages[4]).toMatchObject({ key: 'outcome', status: 'pending' });
  });

  it('marks polling + tar done when archive_sha256 lands and upload starts', () => {
    const stages = deriveStages(
      makeTab({ status: 'downloaded', archive_sha256: 'abc', deploy_status: 'queued' })
    );
    expect(stages[0].status).toBe('done');
    expect(stages[1].status).toBe('done');
    expect(stages[2].status).toBe('active');
  });

  it('marks upload + ci done when deploy outcome succeeds', () => {
    const stages = deriveStages(
      makeTab({
        status: 'closed',
        archive_sha256: 'abc',
        deploy_status: 'succeeded'
      })
    );
    expect(stages[0].status).toBe('done');
    expect(stages[1].status).toBe('done');
    expect(stages[2].status).toBe('done');
    expect(stages[3].status).toBe('done');
    expect(stages[4].status).toBe('done');
  });

  it('marks outcome failed and ci failed on failed-hard outcome', () => {
    const stages = deriveStages(
      makeTab({
        status: 'closed',
        archive_sha256: 'abc',
        deploy_status: 'failed-hard'
      })
    );
    expect(stages[3].status).toBe('failed');
    expect(stages[4].status).toBe('failed');
  });

  it('marks upload failed on upload-sha-mismatch but ci is not failed', () => {
    const stages = deriveStages(
      makeTab({
        status: 'closed',
        archive_sha256: 'abc',
        deploy_status: 'upload-sha-mismatch'
      })
    );
    expect(stages[2].status).toBe('failed');
    expect(stages[4].status).toBe('failed');
  });

  it('isTabClosed only fires when status=closed', () => {
    expect(isTabClosed(makeTab({ status: 'closed' }))).toBe(true);
    expect(isTabClosed(makeTab({ status: 'downloaded' }))).toBe(false);
  });

  it('isTabFailed fires for any failed stage', () => {
    expect(isTabFailed(makeTab({ deploy_status: 'failed-hard', archive_sha256: 'abc' }))).toBe(true);
    expect(isTabFailed(makeTab({ deploy_status: 'succeeded', archive_sha256: 'abc' }))).toBe(false);
  });

  it('isTabPassed fires only when outcome stage is done', () => {
    expect(isTabPassed(makeTab({ deploy_status: 'succeeded', archive_sha256: 'abc' }))).toBe(true);
    expect(isTabPassed(makeTab({ deploy_status: 'running', archive_sha256: 'abc' }))).toBe(false);
  });
});

describe('summarizeOutcome', () => {
  it('extracts outcome fields from deploy-finished event for the tab', () => {
    const summary = summarizeOutcome(
      [
        {
          run_id: 'r',
          tab_id: 4,
          timestamp: '2026-01-01T00:00:00Z',
          kind: 'deploy-finished',
          severity: 'info',
          message: 'deploy done',
          fields: {
            outcome: 'succeeded',
            exit_code: '0',
            remote_command: 'bash ci-fast-push.sh',
            remote_target: 'remote-host:/srv/example-project',
            log_tail: 'lane PASS',
            top_paths: 'src/a.rs,src/b.rs',
            local_sha256: 'aaa',
            remote_sha256: 'bbb'
          }
        }
      ],
      4
    );
    expect(summary.outcome).toBe('succeeded');
    expect(summary.exitCode).toBe('0');
    expect(summary.remoteCommand).toBe('bash ci-fast-push.sh');
    expect(summary.remoteTarget).toBe('remote-host:/srv/example-project');
    expect(summary.logTail).toBe('lane PASS');
    expect(summary.filesChanged).toEqual(['src/a.rs', 'src/b.rs']);
    expect(summary.filesChangedCount).toBeNull();
    expect(summary.additions).toBeNull();
    expect(summary.deletions).toBeNull();
    expect(summary.localSha).toBe('aaa');
    expect(summary.remoteSha).toBe('bbb');
  });

  it('returns empty summary when no deploy-finished event present', () => {
    const summary = summarizeOutcome([], 1);
    expect(summary.outcome).toBe('');
    expect(summary.filesChanged).toEqual([]);
    expect(summary.filesChangedCount).toBeNull();
    expect(summary.logTail).toBeNull();
  });

  it('extracts post_head and ci_state from deploy-finished event', () => {
    const summary = summarizeOutcome(
      [
        {
          run_id: 'r',
          tab_id: 7,
          timestamp: '2026-01-01T00:00:01Z',
          kind: 'deploy-finished',
          severity: 'info',
          message: 'deploy done',
          fields: {
            outcome: 'succeeded',
            post_head: 'df9437530a1110e1a784a53fa7feaefca43383ab',
            ci_state: 'passed'
          }
        }
      ],
      7
    );
    expect(summary.postHead).toBe('df9437530a1110e1a784a53fa7feaefca43383ab');
    expect(summary.ciState).toBe('passed');
  });

  it('parses pre_status and post_status as newline-delimited file lists', () => {
    const summary = summarizeOutcome(
      [
        {
          run_id: 'r',
          tab_id: 2,
          timestamp: '2026-01-01T00:00:02Z',
          kind: 'deploy-finished',
          severity: 'info',
          message: 'deploy done',
          fields: {
            outcome: 'succeeded',
            pre_status: '?? new.rs\nM existing.rs',
            post_status: '   '
          }
        }
      ],
      2
    );
    expect(summary.preStatus).toEqual(['?? new.rs', 'M existing.rs']);
    expect(summary.postStatus).toEqual([]);
  });

  it('prefers changed_paths over top_paths when both present', () => {
    const summary = summarizeOutcome(
      [
        {
          run_id: 'r',
          tab_id: 3,
          timestamp: '2026-01-01T00:00:03Z',
          kind: 'deploy-finished',
          severity: 'info',
          message: 'deploy done',
          fields: {
            outcome: 'succeeded',
            changed_paths: 'crates/x.rs\ncrates/y.rs\ncrates/z.rs',
            top_paths: 'unused/path.rs'
          }
        }
      ],
      3
    );
    expect(summary.filesChanged).toEqual(['crates/x.rs', 'crates/y.rs', 'crates/z.rs']);
  });

  it('captures shortstat when present', () => {
    const summary = summarizeOutcome(
      [
        {
          run_id: 'r',
          tab_id: 5,
          timestamp: '2026-01-01T00:00:04Z',
          kind: 'deploy-finished',
          severity: 'info',
          message: 'deploy done',
          fields: {
            outcome: 'succeeded',
            shortstat: ' 3 files changed, 12 insertions(+), 4 deletions(-)'
          }
        }
      ],
      5
    );
    expect(summary.shortstat).toBe(' 3 files changed, 12 insertions(+), 4 deletions(-)');
    expect(summary.filesChangedCount).toBe(3);
    expect(summary.additions).toBe(12);
    expect(summary.deletions).toBe(4);
  });

  it('prefers deploy change stat fields over shortstat parsing', () => {
    const summary = summarizeOutcome(
      [
        {
          run_id: 'r',
          tab_id: 6,
          timestamp: '2026-01-01T00:00:05Z',
          kind: 'deploy-finished',
          severity: 'info',
          message: 'deploy done',
          fields: {
            outcome: 'succeeded',
            files_changed: '8',
            additions: '99',
            deletions: '7',
            shortstat: ' 3 files changed, 12 insertions(+), 4 deletions(-)'
          }
        }
      ],
      6
    );
    expect(summary.filesChangedCount).toBe(8);
    expect(summary.additions).toBe(99);
    expect(summary.deletions).toBe(7);
  });

  it('parses test counts from local log tail without requiring the field', () => {
    const summary = summarizeOutcome(
      [
        {
          run_id: 'r',
          tab_id: 8,
          timestamp: '2026-01-01T00:00:06Z',
          kind: 'deploy-finished',
          severity: 'info',
          message: 'deploy done',
          fields: {
            outcome: 'succeeded',
            ci_state: 'passed',
            log_tail: 'ci-fast-push: jekko-fast passed\ncargo test: 41 passed\nvitest: 12 passed'
          }
        }
      ],
      8
    );
    expect(summary.localTestsPassed).toBe(53);
    expect(summary.remoteTestsPassed).toBe(53);
  });
});

describe('summarizeRunQuality', () => {
  it('marks an evidence-free run as pending', () => {
    const quality = summarizeRunQuality(
      makeRun({
        tabs: [
          makeTab({ tab_id: 1, deploy_status: 'pending' }),
          makeTab({ tab_id: 2, deploy_status: 'pending' }),
          makeTab({ tab_id: 3, deploy_status: 'pending' })
        ]
      }),
      [],
      []
    );
    expect(quality.verdict).toBe('pending');
    expect(quality.detail).toContain('waiting for evidence');
  });

  it('marks a fully evidenced success as excellent', () => {
    const quality = summarizeRunQuality(
      makeRun({
        finished_at: '2026-01-01T01:00:00Z',
        status: 'finished',
        tabs: [
          makeTab({ tab_id: 1, status: 'closed', archive_sha256: 'abc', deploy_status: 'succeeded' }),
          makeTab({ tab_id: 2, status: 'closed', archive_sha256: 'def', deploy_status: 'succeeded' }),
          makeTab({ tab_id: 3, status: 'closed', archive_sha256: 'ghi', deploy_status: 'succeeded' })
        ]
      }),
      [
        {
          run_id: 'run-1',
          tab_id: 1,
          timestamp: '2026-01-01T00:00:01Z',
          kind: 'download-receipt',
          severity: 'info',
          message: 'receipt confirmed',
          fields: { sha256: 'abc123' }
        },
        {
          run_id: 'run-1',
          tab_id: 1,
          timestamp: '2026-01-01T00:00:02Z',
          kind: 'deploy-finished',
          severity: 'info',
          message: 'deploy done',
          fields: { outcome: 'succeeded' }
        }
      ],
      [{ tab_id: 1, sha256: 'abc123' }]
    );
    expect(quality.verdict).toBe('excellent');
    expect(quality.detail).toContain('passed');
    expect(quality.evidenceKinds).toContain('download-receipt');
    expect(quality.evidenceKinds).toContain('deploy-finished');
  });
});

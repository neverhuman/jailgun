import { describe, expect, it } from 'vitest';

import type { TabSnapshot } from '../types';
import { deriveStages, isTabClosed, isTabFailed, isTabPassed, summarizeOutcome } from './stages';

function makeTab(overrides: Partial<TabSnapshot> = {}): TabSnapshot {
  return {
    tab_id: 1,
    status: 'opening',
    page_url: 'https://chatgpt.com/c/x',
    archive_sha256: null,
    download_latency_ms: null,
    deploy_status: 'pending',
    prompt_policy_decision: null,
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
            files_changed: 'src/a.rs\nsrc/b.rs',
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
    expect(summary.localSha).toBe('aaa');
    expect(summary.remoteSha).toBe('bbb');
  });

  it('returns empty summary when no deploy-finished event present', () => {
    const summary = summarizeOutcome([], 1);
    expect(summary.outcome).toBe('');
    expect(summary.filesChanged).toEqual([]);
    expect(summary.logTail).toBeNull();
  });
});

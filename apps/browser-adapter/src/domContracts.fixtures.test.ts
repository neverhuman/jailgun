import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import { beforeEach, describe, expect, it } from 'vitest';

import {
  collectDismissablePopupFromDom,
  collectGitHubToolPromptsFromDom,
  collectRateLimitModalFromDom,
  collectTarDownloadCandidatesFromDom
} from './domContracts';

const moduleDir = dirname(fileURLToPath(import.meta.url));
const fixtureDir = join(moduleDir, '..', 'test-fixtures', 'chatgpt');

function loadFixture(name: string): void {
  const html = readFileSync(join(fixtureDir, name), 'utf8');
  const bodyMatch = html.match(/<body[^>]*>([\s\S]*?)<\/body>/i);
  if (!bodyMatch) {
    throw new Error(`fixture ${name} missing <body>`);
  }
  document.body.innerHTML = bodyMatch[1];
}

beforeEach(() => {
  document.body.innerHTML = '';
});

describe('chatgpt DOM fixtures — tar download detector', () => {
  it('idle.html → no tar candidates', () => {
    loadFixture('idle.html');
    expect(collectTarDownloadCandidatesFromDom()).toEqual([]);
  });

  it('composing.html → no tar candidates', () => {
    loadFixture('composing.html');
    expect(collectTarDownloadCandidatesFromDom()).toEqual([]);
  });

  it('uploaded-archive.html → no tar candidates (chip is not a download)', () => {
    loadFixture('uploaded-archive.html');
    expect(collectTarDownloadCandidatesFromDom()).toEqual([]);
  });

  it('generating.html → no tar candidates yet', () => {
    loadFixture('generating.html');
    expect(collectTarDownloadCandidatesFromDom()).toEqual([]);
  });

  it('tar-ready-single.html → exactly one tar candidate', () => {
    loadFixture('tar-ready-single.html');
    const candidates = collectTarDownloadCandidatesFromDom();
    expect(candidates).toHaveLength(1);
    expect(candidates[0].download).toBe('jekko-fixes.tar.gz');
  });

  it('tar-ready-multi.html → three candidates ranked equally without targetName', () => {
    loadFixture('tar-ready-multi.html');
    const candidates = collectTarDownloadCandidatesFromDom();
    expect(candidates).toHaveLength(3);
    const scores = candidates.map((candidate) => candidate.score);
    expect(new Set(scores).size).toBe(1);
  });

  it('tar-ready-multi.html → targetName biases jekko-fixes above siblings', () => {
    loadFixture('tar-ready-multi.html');
    const candidates = collectTarDownloadCandidatesFromDom(document, 'jekko-fixes.tar.gz');
    expect(candidates).toHaveLength(3);
    expect(candidates[0].download).toBe('jekko-fixes.tar.gz');
    expect(candidates[0].score).toBeGreaterThan(candidates[1].score);
  });

  it('done-no-tar.html → no tar candidates among final actions', () => {
    loadFixture('done-no-tar.html');
    expect(collectTarDownloadCandidatesFromDom()).toEqual([]);
  });
});

describe('chatgpt DOM fixtures — rate-limit + popup detectors', () => {
  it('rate-limit-modal.html → detector returns the Got it candidate', () => {
    loadFixture('rate-limit-modal.html');
    const candidate = collectRateLimitModalFromDom();
    expect(candidate).not.toBeNull();
    expect(candidate?.buttonLabel).toMatch(/got it/i);
    expect(candidate?.excerpt).toMatch(/too many requests/i);
  });

  it('idle.html → rate-limit detector returns null', () => {
    loadFixture('idle.html');
    expect(collectRateLimitModalFromDom()).toBeNull();
  });

  it('stay-on-page-modal.html → popup detector returns stay-on-page candidate', () => {
    loadFixture('stay-on-page-modal.html');
    const candidates = collectDismissablePopupFromDom();
    expect(candidates).toHaveLength(1);
    expect(candidates[0].kind).toBe('stay-on-page');
    expect(candidates[0].shouldClick).toBe(true);
    expect(candidates[0].buttonLabel).toMatch(/stay/i);
  });

  it('session-expired-modal.html → popup detector returns detect-only candidate', () => {
    loadFixture('session-expired-modal.html');
    const candidates = collectDismissablePopupFromDom();
    expect(candidates).toHaveLength(1);
    expect(candidates[0].kind).toBe('session-expired');
    expect(candidates[0].shouldClick).toBe(false);
  });

  it('rate-limit-modal.html → popup detector ignores rate-limit (handled by separate detector)', () => {
    loadFixture('rate-limit-modal.html');
    expect(collectDismissablePopupFromDom()).toEqual([]);
  });

  it('idle.html → no popup detectable', () => {
    loadFixture('idle.html');
    expect(collectDismissablePopupFromDom()).toEqual([]);
  });
});

describe('chatgpt DOM fixtures — github tool prompt detector', () => {
  it('github-tool-deny.html → returns Deny candidate', () => {
    loadFixture('github-tool-deny.html');
    const candidates = collectGitHubToolPromptsFromDom();
    expect(candidates).toHaveLength(1);
    expect(candidates[0]).toMatchObject({
      action: 'create-tree',
      decision: 'deny',
      label: 'Deny'
    });
  });

  it('github-tool-read.html → returns nothing without allowInfo', () => {
    loadFixture('github-tool-read.html');
    expect(collectGitHubToolPromptsFromDom()).toEqual([]);
  });

  it('github-tool-read.html → returns Allow candidate when allowInfo=true', () => {
    loadFixture('github-tool-read.html');
    const candidates = collectGitHubToolPromptsFromDom(document, true);
    expect(candidates).toHaveLength(1);
    expect(candidates[0]).toMatchObject({
      action: 'read',
      decision: 'allow-info',
      label: 'Allow'
    });
  });

  it('idle.html → no github prompts', () => {
    loadFixture('idle.html');
    expect(collectGitHubToolPromptsFromDom()).toEqual([]);
  });
});

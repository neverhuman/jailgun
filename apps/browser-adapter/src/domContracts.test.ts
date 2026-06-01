import { describe, expect, it } from 'vitest';

import {
  closeTabAfterReceipt,
  collectDismissablePopupFromDom,
  collectGitHubToolPromptsFromDom,
  collectRateLimitModalFromDom,
  collectTarDownloadCandidatesFromDom,
  createPromptClickGuard,
  dismissPopups,
  dismissRateLimitModal
} from './domContracts';

it('finds assistant tar links and ignores user mentions', () => {
  document.body.innerHTML = `
    <div data-message-author-role="user"><a href="https://example.invalid/user.tar.gz">user.tar.gz</a></div>
    <div data-message-author-role="assistant">
      <button download="source.tar.gz">Download source.tar.gz</button>
      <a href="https://example.invalid/notes.md">notes</a>
    </div>
  `;
  const candidates = collectTarDownloadCandidatesFromDom();
  expect(candidates).toHaveLength(1);
  expect(candidates[0].download).toBe('source.tar.gz');
});

it('selects Deny for GitHub Create Tree prompts', () => {
  document.body.innerHTML = `
    <section>
      <p>GitHub tool wants to Create Tree and commit files.</p>
      <button>Deny</button>
      <button>Allow</button>
    </section>
  `;
  const prompts = collectGitHubToolPromptsFromDom();
  expect(prompts[0]).toMatchObject({ action: 'create-tree', decision: 'deny', label: 'Deny', control: 'deny' });
});

it('allows information-only prompts only when policy enables them', () => {
  document.body.innerHTML = `
    <section>
      <p>GitHub tool wants to search repository files.</p>
      <button>Allow</button>
    </section>
  `;
  expect(collectGitHubToolPromptsFromDom()).toHaveLength(0);
  expect(collectGitHubToolPromptsFromDom(document, true)[0]).toMatchObject({
    action: 'search',
    decision: 'allow-info'
  });
});

it('guards repeated prompt scans by signature', () => {
  document.body.innerHTML = '<section><p>GitHub Create Tree</p><button>Deny</button></section>';
  const candidate = collectGitHubToolPromptsFromDom()[0];
  const guard = createPromptClickGuard();
  expect(guard(candidate)).toBe(true);
  expect(guard(candidate)).toBe(false);
});

it('detects the ChatGPT rate-limit modal when phrases and Got it button coexist', () => {
  document.body.innerHTML = `
    <div role="dialog">
      <h2>Too many requests</h2>
      <p>You're making requests too quickly. We've temporarily limited access to your conversations.</p>
      <p>Please wait a few minutes before trying again.</p>
      <button>Got it</button>
    </div>
  `;
  const modal = collectRateLimitModalFromDom();
  expect(modal).not.toBeNull();
  expect(modal?.buttonLabel).toMatch(/got it/i);
  expect(modal?.excerpt).toMatch(/too many requests/i);
  expect(modal?.excerpt).toMatch(/wait a few minutes/i);
});

it('ignores benign popovers whose Got it button is not in a rate-limit dialog', () => {
  document.body.innerHTML = `
    <div role="dialog">
      <p>Welcome to ChatGPT! Take the tour to learn more.</p>
      <button>Got it</button>
    </div>
  `;
  expect(collectRateLimitModalFromDom()).toBeNull();
});

it('ignores rate-limit phrases when no Got it button is present', () => {
  document.body.innerHTML = `
    <div role="dialog">
      <h2>Too many requests</h2>
      <p>Please wait a few minutes before trying again.</p>
      <button>Close</button>
    </div>
  `;
  expect(collectRateLimitModalFromDom()).toBeNull();
});

it('ignores hidden rate-limit dialogs', () => {
  document.body.innerHTML = `
    <div role="dialog" style="display:none">
      <p>Too many requests</p>
      <p>Please wait a few minutes before trying again.</p>
      <button>Got it</button>
    </div>
  `;
  expect(collectRateLimitModalFromDom()).toBeNull();
});

it('dismissRateLimitModal returns detected=false when no dialog is present', async () => {
  document.body.innerHTML = '<main><p>nothing here</p></main>';
  const fakePage = { evaluate: async <T,>(fn: () => T) => fn() };
  const result = await dismissRateLimitModal(fakePage);
  expect(result.detected).toBe(false);
  expect(result.dismissed).toBe(false);
});

it('dismissRateLimitModal clicks the Got it button when a rate-limit dialog is present', async () => {
  document.body.innerHTML = `
    <div role="dialog">
      <p>Too many requests. Please wait a few minutes before trying again.</p>
      <button id="ack">Got it</button>
    </div>
  `;
  const clicks: string[] = [];
  document.getElementById('ack')!.addEventListener('click', () => {
    clicks.push('got-it');
  });
  const fakePage = { evaluate: async <T,>(fn: () => T) => fn() };
  const result = await dismissRateLimitModal(fakePage);
  expect(result.detected).toBe(true);
  expect(result.dismissed).toBe(true);
  expect(result.buttonLabel).toMatch(/got it/i);
  expect(clicks).toEqual(['got-it']);
});

it('detects the stay-on-page popup and dismissPopups clicks Stay', async () => {
  document.body.innerHTML = `
    <div role="dialog">
      <p>Leave this page? Changes you've made may not be saved.</p>
      <button id="leave">Leave</button>
      <button id="stay">Stay on this page</button>
    </div>
  `;
  const candidates = collectDismissablePopupFromDom();
  expect(candidates).toHaveLength(1);
  expect(candidates[0].kind).toBe('stay-on-page');
  expect(candidates[0].shouldClick).toBe(true);
  expect(candidates[0].buttonLabel).toMatch(/stay/i);

  const clicks: string[] = [];
  document.getElementById('stay')!.addEventListener('click', () => clicks.push('stay'));
  document.getElementById('leave')!.addEventListener('click', () => clicks.push('leave'));

  const fakePage = { evaluate: async <T,>(fn: () => T) => fn() };
  const outcomes = await dismissPopups(fakePage);
  expect(outcomes).toHaveLength(1);
  expect(outcomes[0].kind).toBe('stay-on-page');
  expect(outcomes[0].clicked).toBe(true);
  expect(clicks).toEqual(['stay']);
});

it('detects session-expired popup but does not auto-click anything', async () => {
  document.body.innerHTML = `
    <div role="dialog">
      <h2>Session expired</h2>
      <p>Please sign in again to continue.</p>
      <button id="signin">Sign in</button>
    </div>
  `;
  const candidates = collectDismissablePopupFromDom();
  expect(candidates).toHaveLength(1);
  expect(candidates[0].kind).toBe('session-expired');
  expect(candidates[0].shouldClick).toBe(false);
  expect(candidates[0].buttonLabel).toBe('');

  const clicks: string[] = [];
  document.getElementById('signin')!.addEventListener('click', () => clicks.push('signin'));

  const fakePage = { evaluate: async <T,>(fn: () => T) => fn() };
  const outcomes = await dismissPopups(fakePage);
  expect(outcomes).toHaveLength(1);
  expect(outcomes[0].kind).toBe('session-expired');
  expect(outcomes[0].clicked).toBe(false);
  expect(clicks).toEqual([]);
});

it('ignores benign onboarding dialogs that do not match any popup recipe', async () => {
  document.body.innerHTML = `
    <div role="dialog">
      <h2>Welcome to ChatGPT</h2>
      <p>Take the tour to learn about new features.</p>
      <button>Got it</button>
      <button>Dismiss</button>
    </div>
  `;
  expect(collectDismissablePopupFromDom()).toEqual([]);
  const fakePage = { evaluate: async <T,>(fn: () => T) => fn() };
  expect(await dismissPopups(fakePage)).toEqual([]);
});

it('closes a tab only after receipt confirmation', async () => {
  const calls: string[] = [];
  const page = {
    locator: () => ({
      count: async () => 1,
      click: async () => {
        calls.push('stop');
      }
    }),
    close: async () => {
      calls.push('close');
    }
  };
  expect(await closeTabAfterReceipt(page, false)).toBe(false);
  expect(calls).toEqual([]);
  expect(await closeTabAfterReceipt(page, true)).toBe(true);
  expect(calls).toEqual(['stop', 'close']);
});

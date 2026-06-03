import { describe, expect, it } from 'vitest';

import {
  clickGitHubToolPrompt,
  closeTabAfterReceipt,
  collectDismissablePopupFromDom,
  collectGitHubToolPromptsFromDom,
  collectRateLimitModalFromDom,
  collectTarDownloadCandidatesFromDom,
  createPromptClickGuard,
  detectABFeedbackFromDom,
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

it('biases tar candidates toward --tar-target-name when multiple .tar.gz links appear', () => {
  document.body.innerHTML = `
    <div data-message-author-role="assistant">
      <a href="https://example.invalid/jekko.tar.gz" download="jekko.tar.gz">Download jekko.tar.gz</a>
      <a href="https://example.invalid/jekko-fixes.tar.gz" download="jekko-fixes.tar.gz">Download jekko-fixes.tar.gz</a>
      <a href="https://example.invalid/dummy.tar.gz" download="dummy.tar.gz">Download dummy.tar.gz</a>
    </div>
  `;
  const withTarget = collectTarDownloadCandidatesFromDom(document, 'jekko-fixes.tar.gz');
  expect(withTarget).toHaveLength(3);
  expect(withTarget[0].download).toBe('jekko-fixes.tar.gz');
  expect(withTarget[0].score).toBeGreaterThan(withTarget[1].score);

  const noTarget = collectTarDownloadCandidatesFromDom(document);
  expect(noTarget).toHaveLength(3);
  expect(noTarget.map((candidate) => candidate.download)).toContain('jekko-fixes.tar.gz');
  expect(noTarget[0].score).toBe(noTarget[1].score);
});

it('preserves existing tar candidate ordering when targetName is omitted', () => {
  document.body.innerHTML = `
    <div data-message-author-role="assistant">
      <a href="https://example.invalid/a.tar.gz" download="a.tar.gz">Download a.tar.gz</a>
      <button download="b.tar.gz">Download b.tar.gz</button>
    </div>
  `;
  const candidates = collectTarDownloadCandidatesFromDom(document);
  expect(candidates).toHaveLength(2);
  const baselineOrder = candidates.map((candidate) => candidate.download);
  const repeat = collectTarDownloadCandidatesFromDom(document);
  expect(repeat.map((candidate) => candidate.download)).toEqual(baselineOrder);
  expect(candidates[0].score).toBeGreaterThanOrEqual(candidates[1].score);
});

it('clickGitHubToolPrompt clicks the Nth control and returns observed label', async () => {
  document.body.innerHTML = `
    <button>Allow</button>
    <button>Deny</button>
  `;
  const clicks: string[] = [];
  document.querySelectorAll('button').forEach((button) => {
    button.addEventListener('click', () => clicks.push(button.textContent ?? ''));
  });
  const denyCandidate = collectGitHubToolPromptsFromDom(document)[0] ?? null;
  const fakePage = {
    locator: (selector: string) => {
      const matches = Array.from(document.querySelectorAll(selector)) as HTMLElement[];
      return {
        nth: (index: number) => {
          const element = matches[index];
          return {
            count: async () => (element ? 1 : 0),
            isVisible: async () => Boolean(element),
            isEnabled: async () => Boolean(element) && !element.hasAttribute('disabled'),
            textContent: async () => element?.textContent ?? null,
            getAttribute: async (name: string) => element?.getAttribute(name) ?? null,
            click: async () => {
              element?.click();
            }
          };
        }
      };
    }
  };
  const denyDefault = { index: 1, label: 'Deny' } as Parameters<typeof clickGitHubToolPrompt>[1];
  const result = await clickGitHubToolPrompt(fakePage, denyCandidate ?? denyDefault);
  expect(result.clicked).toBe(true);
  expect(result.label).toMatch(/deny/i);
  expect(clicks).toEqual(['Deny']);
});

it('clickGitHubToolPrompt reports not-found when index is out of range', async () => {
  document.body.innerHTML = '<button>Allow</button>';
  const fakePage = {
    locator: () => ({
      nth: () => ({
        count: async () => 0,
        isVisible: async () => false,
        isEnabled: async () => false,
        textContent: async () => null,
        getAttribute: async () => null,
        click: async () => undefined
      })
    })
  };
  const result = await clickGitHubToolPrompt(fakePage, { index: 5, label: 'Deny' } as Parameters<typeof clickGitHubToolPrompt>[1]);
  expect(result.clicked).toBe(false);
  expect(result.reason).toBe('not-found');
});

it('clickGitHubToolPrompt reports disabled when button is disabled', async () => {
  document.body.innerHTML = '<button disabled>Deny</button>';
  const fakePage = {
    locator: () => ({
      nth: () => ({
        count: async () => 1,
        isVisible: async () => true,
        isEnabled: async () => false,
        textContent: async () => 'Deny',
        getAttribute: async (name: string) => (name === 'aria-disabled' ? null : null),
        click: async () => undefined
      })
    })
  };
  const result = await clickGitHubToolPrompt(fakePage, { index: 0, label: 'Deny' } as Parameters<typeof clickGitHubToolPrompt>[1]);
  expect(result.clicked).toBe(false);
  expect(result.reason).toBe('disabled');
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

it('detectABFeedbackFromDom returns detected:false when no feedback text exists', () => {
  document.body.innerHTML = `
    <div data-message-author-role="assistant">
      <p>Here is your source archive.</p>
      <a href="https://example.invalid/source.tar.gz" download="source.tar.gz">Download source.tar.gz</a>
    </div>
  `;
  const state = detectABFeedbackFromDom();
  expect(state.detected).toBe(false);
  expect(state.responseCount).toBe(0);
  expect(state.responses).toEqual([]);
  expect(state.longestIndex).toBeNull();
});

it('detectABFeedbackFromDom returns detected:true when feedback text exists', () => {
  document.body.innerHTML = `
    <div>
      <p>You're giving feedback on a new version of ChatGPT.</p>
      <p>Which response do you prefer?</p>
      <div data-testid="response-turn-0">
        <h3>Response 1</h3>
        <p>Short answer.</p>
      </div>
      <div data-testid="response-turn-1">
        <h3>Response 2</h3>
        <p>This is a much longer answer with more detail and explanation about the topic at hand.</p>
      </div>
    </div>
  `;
  const state = detectABFeedbackFromDom();
  expect(state.detected).toBe(true);
  expect(state.responseCount).toBe(2);
  expect(state.responses).toHaveLength(2);
  expect(state.longestIndex).toBe(1);
  expect(state.responses[1].textLength).toBeGreaterThan(state.responses[0].textLength);
});

it('collectTarDownloadCandidatesFromDom finds tar links inside A/B response containers', () => {
  document.body.innerHTML = `
    <div>
      <p>You're giving feedback on a new version of ChatGPT.</p>
      <div data-testid="response-turn-0">
        <p>Response A content</p>
      </div>
      <div data-testid="response-turn-1">
        <p>Response B content</p>
        <a href="https://example.invalid/output.tar.gz" download="output.tar.gz">Download output.tar.gz</a>
      </div>
    </div>
  `;
  const candidates = collectTarDownloadCandidatesFromDom();
  expect(candidates).toHaveLength(1);
  expect(candidates[0].download).toBe('output.tar.gz');
});

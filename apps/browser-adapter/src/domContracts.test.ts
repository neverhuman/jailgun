import { describe, expect, it } from 'vitest';

import {
  clickGitHubToolPrompt,
  closeTabAfterReceipt,
  collectDismissablePopupFromDom,
  collectArtifactConversationLinksFromDom,
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

it('finds assistant tar links with numbered names', () => {
  document.body.innerHTML = `
    <div data-message-author-role="assistant">
      <a href="https://example.invalid/source.tar(289).gz" download="source.tar(289).gz">
        Download source.tar(289).gz
      </a>
    </div>
  `;
  const candidates = collectTarDownloadCandidatesFromDom();
  expect(candidates).toHaveLength(1);
  expect(candidates[0].download).toBe('source.tar(289).gz');
});

it('prefers assistant tex downloads when a tex target is requested', () => {
  document.body.innerHTML = `
    <div data-message-author-role="assistant">
      <a href="https://example.invalid/chapter-033-epoch-02.tar.gz" download="chapter-033-epoch-02.tar.gz">
        Download alternate archive
      </a>
      <a href="https://example.invalid/chapter-033-epoch-02.tex" download="chapter-033-epoch-02.tex">
        Download chapter-033-epoch-02.tex
      </a>
    </div>
  `;
  const candidates = collectTarDownloadCandidatesFromDom(document, 'chapter-033-epoch-02.tex');
  expect(candidates).toHaveLength(2);
  expect(candidates[0]).toMatchObject({
    download: 'chapter-033-epoch-02.tex',
    fileKind: 'downloaded-tex'
  });
});

it('finds exact assistant json download targets without requiring tar.gz', () => {
  document.body.innerHTML = `
    <div data-message-author-role="assistant">
      <a href="https://files.example.invalid/openqg-smoke.json" download="openqg-smoke.json">
        Download openqg-smoke.json
      </a>
    </div>
  `;
  const candidates = collectTarDownloadCandidatesFromDom(document, 'openqg-smoke.json');
  expect(candidates).toHaveLength(1);
  expect(candidates[0]).toMatchObject({
    download: 'openqg-smoke.json',
    fileKind: 'downloaded-file'
  });
});

it('finds exact assistant download targets with arbitrary extensions', () => {
  document.body.innerHTML = `
    <div data-message-author-role="assistant">
      <a href="https://files.example.invalid/openqg-smoke.qg" download="openqg-smoke.qg">
        Download openqg-smoke.qg
      </a>
    </div>
  `;
  const candidates = collectTarDownloadCandidatesFromDom(document, 'openqg-smoke.qg');
  expect(candidates).toHaveLength(1);
  expect(candidates[0]).toMatchObject({
    download: 'openqg-smoke.qg',
    fileKind: 'downloaded-file'
  });
});

it('accepts generic artifact export controls for known non-archive targets', () => {
  document.body.innerHTML = `
    <div data-message-author-role="assistant">
      <button aria-label="Download artifact">Download</button>
    </div>
  `;
  const candidates = collectTarDownloadCandidatesFromDom(document, 'openqg-smoke.csv');
  expect(candidates).toHaveLength(1);
  expect(candidates[0]).toMatchObject({
    aria: 'Download artifact',
    fileKind: 'downloaded-file'
  });
});

it('rejects explicit wrong filenames for non-archive targets', () => {
  document.body.innerHTML = `
    <div data-message-author-role="assistant">
      <a href="https://files.example.invalid/wrong.json" download="wrong.json">
        Download wrong.json
      </a>
    </div>
  `;
  expect(collectTarDownloadCandidatesFromDom(document, 'openqg-smoke.json')).toEqual([]);
});

it('rejects explicit wrong filenames with arbitrary extensions', () => {
  document.body.innerHTML = `
    <div data-message-author-role="assistant">
      <button>Download wrong.qg</button>
    </div>
  `;
  expect(collectTarDownloadCandidatesFromDom(document, 'openqg-smoke.qg')).toEqual([]);
});

it('ignores prompt-side upload chips that mention tar archives', () => {
  document.body.innerHTML = `
    <form>
      <div data-testid="upload-chip" aria-label="Attached source.tar(289).gz">
        <button type="button" aria-label="Remove attachment">source.tar(289).gz</button>
        <span data-state="ready">Attached</span>
      </div>
    </form>
  `;
  expect(collectTarDownloadCandidatesFromDom()).toEqual([]);
});

it('collects same-chapter artifact conversation links for tar recovery', () => {
  document.body.innerHTML = `
    <nav>
      <a href="https://chatgpt.com/c/chapter-027-artifact?model=gpt-5">
        Chapter 027 Tar.gz
      </a>
    </nav>
  `;
  const links = collectArtifactConversationLinksFromDom(
    document,
    'chapter-027-epoch-02.tar.gz',
    'https://chatgpt.com/c/current-conversation?locale=en-US'
  );
  expect(links).toHaveLength(1);
  expect(links[0]).toMatchObject({
    url: 'https://chatgpt.com/c/chapter-027-artifact',
    text: 'Chapter 027 Tar.gz',
    chapter: '27',
    targetMatched: true
  });
});

it('ignores mismatched artifact conversation chapters', () => {
  document.body.innerHTML = `
    <nav>
      <a href="https://chatgpt.com/c/chapter-028-artifact">Chapter 028 Tar.gz</a>
    </nav>
  `;
  expect(collectArtifactConversationLinksFromDom(
    document,
    'chapter-027-epoch-02.tar.gz',
    'https://chatgpt.com/c/current-conversation'
  )).toEqual([]);
});

it('ignores generic editorial conversation links even when the chapter matches', () => {
  document.body.innerHTML = `
    <nav>
      <a href="https://chatgpt.com/c/chapter-027-review">Chapter 027 Editorial Review</a>
    </nav>
  `;
  expect(collectArtifactConversationLinksFromDom(
    document,
    'chapter-027-epoch-02.tar.gz',
    'https://chatgpt.com/c/current-conversation'
  )).toEqual([]);
});

it('ignores upload chips and conversation option controls as artifact conversation links', () => {
  document.body.innerHTML = `
    <form>
      <div data-testid="upload-chip">
        <a href="https://chatgpt.com/c/chapter-027-artifact">Chapter 027 Tar.gz</a>
      </div>
    </form>
    <button aria-label="Open conversation options for Chapter 027 Tar.gz"></button>
  `;
  expect(collectArtifactConversationLinksFromDom(
    document,
    'chapter-027-epoch-02.tar.gz',
    'https://chatgpt.com/c/current-conversation'
  )).toEqual([]);
});

it('dedupes artifact conversation links and excludes the current conversation', () => {
  document.body.innerHTML = `
    <nav>
      <a href="https://chatgpt.com/c/current-conversation?foo=1">Chapter 027 Tar.gz</a>
      <a href="https://chatgpt.com/c/chapter-027-artifact?model=gpt-5">Chapter 027 Artifact</a>
      <a href="https://chatgpt.com/en/c/chapter-027-artifact?locale=en-US">Chapter 027 LaTeX Creation</a>
    </nav>
  `;
  const links = collectArtifactConversationLinksFromDom(
    document,
    'chapter-027-epoch-02.tar.gz',
    'https://chatgpt.com/c/current-conversation?locale=en-US'
  );
  expect(links).toHaveLength(1);
  expect(links[0].url).toBe('https://chatgpt.com/c/chapter-027-artifact');
  expect(links[0].score).toBeGreaterThan(0);
});

it('collects artifact conversation links for exact non-archive target names', () => {
  document.body.innerHTML = `
    <nav>
      <a href="https://chatgpt.com/c/openqg-json-artifact">
        openqg-smoke.json
      </a>
    </nav>
  `;
  const links = collectArtifactConversationLinksFromDom(
    document,
    'openqg-smoke.json',
    'https://chatgpt.com/c/current-conversation'
  );
  expect(links).toHaveLength(1);
  expect(links[0]).toMatchObject({
    url: 'https://chatgpt.com/c/openqg-json-artifact',
    targetMatched: true
  });
  expect(links[0].artifactSignals).toContain('artifact-name');
});

it('does not treat ChatGPT conversation artifact links as direct tar downloads', () => {
  document.body.innerHTML = `
    <nav>
      <a href="https://chatgpt.com/c/chapter-027-artifact">Chapter 027 Tar.gz</a>
    </nav>
  `;
  expect(collectTarDownloadCandidatesFromDom(document, 'chapter-027-epoch-02.tar.gz')).toEqual([]);
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

it('detects assistant download controls that name a tarball without a filename', () => {
  document.body.innerHTML = `
    <div data-message-author-role="assistant">
      <button>Download the tarball</button>
    </div>
  `;
  const candidates = collectTarDownloadCandidatesFromDom(document);
  expect(candidates).toHaveLength(1);
  expect(candidates[0]).toMatchObject({
    text: 'Download the tarball',
    scope: 'assistant',
    tagName: 'button'
  });
});

it('ignores filename-only A/B feedback buttons until a response is selected', () => {
  document.body.innerHTML = `
    <div data-message-author-role="assistant">
      <div>You're giving feedback on a new version of ChatGPT.</div>
      <div>Which response do you prefer?</div>
      <div data-paragen-root="true">
        <button>03-agent-arrives-job-004-zyal.tar.gz</button>
        <button>I prefer this response</button>
      </div>
      <div data-paragen-root="true">
        <p>I prefer this response</p>
      </div>
    </div>
  `;
  expect(collectTarDownloadCandidatesFromDom(document, '03-agent-arrives-job-004-zyal.tar.gz')).toEqual([]);
});

it('ignores filename-only A/B feedback buttons without relying on paragen markup', () => {
  document.body.innerHTML = `
    <div data-message-author-role="assistant">
      <div>Which response do you prefer?</div>
      <div data-testid="response-turn-1">
        <button>03-agent-arrives-job-004-zyal.tar.gz</button>
      </div>
    </div>
  `;
  expect(collectTarDownloadCandidatesFromDom(document, '03-agent-arrives-job-004-zyal.tar.gz')).toEqual([]);
});

it('ranks explicit .tar.gz links above generic archive download buttons', () => {
  document.body.innerHTML = `
    <div data-message-author-role="assistant">
      <button>Download the tarball</button>
      <a href="https://example.invalid/source.tar.gz" download="source.tar.gz">Download source.tar.gz</a>
    </div>
  `;
  const candidates = collectTarDownloadCandidatesFromDom(document);
  expect(candidates).toHaveLength(2);
  expect(candidates[0].download).toBe('source.tar.gz');
  expect(candidates[0].score).toBeGreaterThan(candidates[1].score);
});

it('ignores ChatGPT history links whose only tar signal is the label', () => {
  document.body.innerHTML = `
    <nav>
      <a href="https://chatgpt.com/c/6a224b5f-25f0-83e8-8556-b960941c7551" aria-label="Missing .tar.gz Archive, unread">
        Missing .tar.gz Archive
      </a>
      <button aria-label="Open conversation options for Missing .tar.gz Archive"></button>
    </nav>
  `;
  expect(collectTarDownloadCandidatesFromDom(document)).toEqual([]);
});

it('ignores generic document download controls outside assistant messages', () => {
  document.body.innerHTML = `
    <aside>
      <button>Download the archive</button>
      <a href="https://chatgpt.com/c/6a224b5f-25f0-83e8-8556-b960941c7551" aria-label="Download Missing .tar.gz Archive">
        Download Missing .tar.gz Archive
      </a>
    </aside>
  `;
  expect(collectTarDownloadCandidatesFromDom(document)).toEqual([]);
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

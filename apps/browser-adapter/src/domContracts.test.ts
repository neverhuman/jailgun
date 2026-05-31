import { describe, expect, it } from 'vitest';

import {
  closeTabAfterReceipt,
  collectGitHubToolPromptsFromDom,
  collectTarDownloadCandidatesFromDom,
  createPromptClickGuard
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

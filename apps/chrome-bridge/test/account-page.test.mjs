import assert from 'node:assert/strict';
import test from 'node:test';
import { selectAccountPage } from '../src/account-page.mjs';

function page(url, { composer = false, closed = false } = {}) {
  return {
    url: () => url,
    isClosed: () => closed,
    locator: () => ({ first: () => ({ isVisible: async () => composer }) }),
  };
}

test('selects replacement ChatGPT composer after original login tab closes', async () => {
  const original = page('https://chatgpt.com/auth/login', { closed: true });
  const signedIn = page('https://chatgpt.com/', { composer: true });
  const context = { pages: () => [original, signedIn], newPage: async () => page('about:blank') };
  assert.equal(await selectAccountPage(context, original, 'https://chatgpt.com'), signedIn);
});

test('prefers provider page over unrelated live tabs', async () => {
  const unrelated = page('https://example.com/');
  const provider = page('https://chatgpt.com/');
  const context = { pages: () => [unrelated, provider], newPage: async () => page('about:blank') };
  assert.equal(await selectAccountPage(context, unrelated, 'https://chatgpt.com'), provider);
});

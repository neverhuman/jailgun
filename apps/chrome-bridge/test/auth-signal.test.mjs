import assert from 'node:assert/strict';
import test from 'node:test';
import { authenticatedComposerState, readAuthenticatedIdentity, uniqueEmailFromTexts } from '../src/auth-signal.mjs';

test('an anonymous composer is not an authenticated session', () => {
  assert.equal(authenticatedComposerState({ composerDetected: true }).state, 'auth-required');
  assert.equal(authenticatedComposerState({ composerDetected: true, identity: { id: 'user', email: 'synthetic@example.invalid' }, loginVisible: true }).state, 'auth-required');
});

test('identity must match the account binding', () => {
  const identity = { id: 'user', email: 'synthetic@example.invalid' };
  assert.equal(authenticatedComposerState({ composerDetected: true, identity, expectedEmail: 'other@example.invalid' }).state, 'account-mismatch');
  assert.deepEqual(authenticatedComposerState({ composerDetected: true, identity, expectedEmail: 'SYNTHETIC@example.invalid' }), { state: 'ready', identity, reason: null });
});

test('session projection never returns access tokens', async () => {
  const originalFetch = globalThis.fetch;
  try {
    globalThis.fetch = async () => ({ ok: true, json: async () => ({ accessToken: String(123), user: { id: 'synthetic-id', email: 'synthetic@example.invalid', secret: 'private' } }) });
    const identity = await readAuthenticatedIdentity({ evaluate: fn => fn() });
    assert.deepEqual(identity, { id: 'synthetic-id', email: 'synthetic@example.invalid' });
    globalThis.fetch = async () => { throw new Error('unavailable'); };
    assert.equal(await readAuthenticatedIdentity({ evaluate: fn => fn() }), null);
  } finally { globalThis.fetch = originalFetch; }
});

test('visible account identity requires one unambiguous email', () => {
  assert.equal(uniqueEmailFromTexts(['Ben Example', 'ben@example.com']), 'ben@example.com');
  assert.equal(uniqueEmailFromTexts(['ben@example.com', 'BEN@example.com']), 'ben@example.com');
  assert.equal(uniqueEmailFromTexts(['ben@example.com', 'other@example.com']), null);
  assert.equal(uniqueEmailFromTexts(['No email here']), null);
});

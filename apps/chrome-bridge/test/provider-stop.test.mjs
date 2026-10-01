import assert from 'node:assert/strict';
import test from 'node:test';
import { ChatGPTProvider, modelMenuItemState, resolveModelLabel } from '../src/chatgpt-provider.mjs';

const turn = { conversation_url: 'http://127.0.0.1:9999/c/owned', user_turn_id: 'owned-turn' };

test('a failed Stop click still verifies natural completion of the owned turn', async () => {
  let streaming = true;
  let clicks = 0;
  const button = { isVisible: async () => true, click: async () => {
    clicks++;streaming = false;throw new Error('Stop became hidden while clicking');
  } };
  const page = { url: () => turn.conversation_url, locator: () => ({ first: () => button }), evaluate: async (_, args) => {
    assert.equal(args.userTurnId, 'owned-turn');return { streaming, error: null };
  } };
  const provider = new ChatGPTProvider(page, { baseUrl: 'http://127.0.0.1:9999', pollMs: 1 });
  provider.activeTurn = turn;
  await provider.stop();
  assert.equal(clicks, 1);
});

test('missing turn ownership cannot be acknowledged as a successful stop', async () => {
  let clicked = false;
  const page = { url: () => turn.conversation_url, locator: () => ({ first: () => ({
    isVisible: async () => true, click: async () => { clicked = true; },
  }) }), evaluate: async () => ({ streaming: false, error: 'adapter-turn-missing' }) };
  const provider = new ChatGPTProvider(page, { baseUrl: 'http://127.0.0.1:9999' });
  provider.activeTurn = turn;
  await assert.rejects(provider.stop(), { code: 'adapter-turn-missing' });
  assert.equal(clicked, false);
});

test('capture accepts verified local-chatgpt route canonicalization', async () => {
  const localTurn = {
    conversation_url: 'http://127.0.0.1:9999/c/local-chatgpt%3Atemporary',
    user_turn_id: 'owned-turn',
    observed_model: 'current',
  };
  const page = {
    url: () => 'http://127.0.0.1:9999/c/durable-conversation',
    evaluate: async (_, args) => args?.userTurnId ? {
      error: null,
      markdown: 'JAILGUN_OK',
      assistantId: 'assistant-turn',
      completionSignal: true,
      streaming: false,
      rateLimited: false,
      expired: false,
      observedModel: 'current',
    } : { id: 'provider-account', email: 'worker@example.com' },
  };
  const provider = new ChatGPTProvider(page, {
    baseUrl: 'http://127.0.0.1:9999',
    identity: { id: 'provider-account', email: 'worker@example.com' },
    pollMs: 1,
    settleMs: 0,
  });
  const result = await provider.capture(localTurn);
  assert.equal(result.complete, true);
  assert.equal(result.markdown, 'JAILGUN_OK');
});

test('capture rejects a change between durable conversation routes', async () => {
  const page = { url: () => 'http://127.0.0.1:9999/c/other' };
  const provider = new ChatGPTProvider(page, { baseUrl: 'http://127.0.0.1:9999' });
  const result = await provider.capture(turn);
  assert.equal(result.complete, false);
  assert.equal(result.error_code, 'adapter-conversation-changed');
});

test('portable model aliases resolve one observed versioned label', () => {
  const available = ['GPT-5.6 Sol', 'GPT-5.5', 'Astra'];
  assert.equal(resolveModelLabel('sol', available), 'GPT-5.6 Sol');
  assert.equal(resolveModelLabel('astra', available), 'Astra');
  assert.equal(resolveModelLabel('GPT-5.5', available), 'GPT-5.5');
  assert.equal(resolveModelLabel('unknown', available), null);
  assert.equal(resolveModelLabel('sol', ['Sol latest', 'Sol legacy']), null);
});

test('model confirmation survives menu rows inserted after selection', () => {
  const node = (label, checked = null) => ({
    textContent: label,
    clicked: false,
    getAttribute(name) { return name === 'aria-checked' ? checked : null; },
    click() { this.clicked = true; },
  });
  const sol = node('GPT-5.6 Sol', 'true');
  const rows = [node('Select model'), node('Reset to default'), node('Power'), node('Latest', 'false'), sol];
  assert.deepEqual(modelMenuItemState(rows, { label: 'GPT-5.6 Sol', click: true }), { found: true, checked: true });
  assert.equal(sol.clicked, true);
});

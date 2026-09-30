import assert from 'node:assert/strict';
import test from 'node:test';
import { ChatGPTProvider } from '../src/chatgpt-provider.mjs';

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

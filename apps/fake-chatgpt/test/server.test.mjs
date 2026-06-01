import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { start } from '../src/server.mjs';

let handle;

beforeEach(async () => {
  handle = await start({ port: 0 });
});

afterEach(async () => {
  if (handle) {
    await handle.stop();
    handle = null;
  }
});

async function postJson(path, payload) {
  const res = await fetch(`${handle.url}${path}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(payload),
  });
  return { status: res.status, body: await res.json() };
}

async function getJson(path) {
  const res = await fetch(`${handle.url}${path}`);
  return { status: res.status, body: await res.json() };
}

async function getHtml(path) {
  const res = await fetch(`${handle.url}${path}`);
  return { status: res.status, body: await res.text() };
}

describe('fake-chatgpt server', () => {
  it('serves the landing JSON at /', async () => {
    const { status, body } = await getJson('/');
    expect(status).toBe(200);
    expect(body.service).toBe('fake-chatgpt');
  });

  it('serves the idle fixture body at /c/:id by default', async () => {
    const { status, body } = await getHtml('/c/test-conversation');
    expect(status).toBe(200);
    expect(body).toMatch(/Start a new conversation/);
    expect(body).toMatch(/<meta name="fake-chatgpt-state" content="idle"/);
  });

  it('admin/state transitions the conversation state', async () => {
    const set = await postJson('/admin/state', {
      conversation_id: 'tab-1',
      state: 'tar-ready',
    });
    expect(set.status).toBe(200);
    expect(set.body.state).toBe('tar-ready');

    const page = await getHtml('/c/tab-1');
    expect(page.status).toBe(200);
    expect(page.body).toMatch(/jekko-fixes\.tar\.gz/);
  });

  it('admin/state with overlays renders modal HTML', async () => {
    await postJson('/admin/state', {
      conversation_id: 'tab-2',
      state: 'idle',
      overlays: ['rate_limit'],
    });
    const page = await getHtml('/c/tab-2');
    expect(page.body).toMatch(/Too many requests/);
    expect(page.body).toMatch(/Got it/);
  });

  it('admin/advance walks the state machine forward', async () => {
    const first = await postJson('/admin/advance', { conversation_id: 'tab-3' });
    expect(first.body.state).toBe('composing');
    const second = await postJson('/admin/advance', { conversation_id: 'tab-3' });
    expect(second.body.state).toBe('uploading');
  });

  it('admin/reset clears all state', async () => {
    await postJson('/admin/state', { conversation_id: 'tab-4', state: 'done' });
    const before = await getJson('/admin/status');
    expect(before.body.conversations).toHaveLength(1);
    const reset = await postJson('/admin/reset', {});
    expect(reset.status).toBe(200);
    const after = await getJson('/admin/status');
    expect(after.body.conversations).toEqual([]);
  });

  it('admin/state rejects invalid state names', async () => {
    const { status, body } = await postJson('/admin/state', {
      conversation_id: 'tab-5',
      state: 'banana',
    });
    expect(status).toBe(500);
    expect(body.error).toMatch(/invalid state/i);
  });

  it('tar-ready with multi target name serves multi-tar fixture', async () => {
    await postJson('/admin/state', {
      conversation_id: 'tab-6',
      state: 'tar-ready',
      tar_target_name: 'jekko-fixes-multi.tar.gz',
    });
    const page = await getHtml('/c/tab-6');
    expect(page.body).toMatch(/jekko\.tar\.gz/);
    expect(page.body).toMatch(/jekko-fixes\.tar\.gz/);
    expect(page.body).toMatch(/dummy\.tar\.gz/);
  });

  it('404s for unknown routes', async () => {
    const { status } = await getJson('/admin/unknown');
    expect(status).toBe(404);
  });
});

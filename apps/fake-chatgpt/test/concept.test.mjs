import { expect, it } from 'vitest';
import { start } from '../src/server.mjs';

it('keeps isolated login identities, expiry, models and conversation ownership separate', async () => {
  const server = await start({ port: 0, interactive: true });
  const request = (path, cookie, body) => fetch(`${server.url}${path}`, {
    method: body ? 'POST' : 'GET', headers: { ...(cookie ? { cookie } : {}), 'content-type': 'application/json' },
    ...(body ? { body: JSON.stringify(body) } : {}), signal: AbortSignal.timeout(5000),
  });
  try {
    server.concept.controls.isolatedAccounts = true;
    const identities = ['first', 'second'].map(id => ({ id, email: `${id}@example.invalid` }));
    const cookies = [];
    for (const identity of identities) {
      server.concept.controls.account = identity;
      const login = await request('/api/test-login', null, {});
      cookies.push(login.headers.getSetCookie()[0].split(';')[0]);
    }
    expect(cookies[0]).not.toBe(cookies[1]);
    for (let index = 0; index < 2; index++) {
      expect((await (await request('/api/auth/session', cookies[index])).json()).user).toEqual(identities[index]);
    }
    const conversation = await (await request('/api/conversation', cookies[0], { prompt: 'Synthetic fixture', model: 'Fixture One' })).json();
    expect((await request(`/api/conversation/${conversation.id}`, cookies[1])).status).toBe(404);
    expect((await request(`/api/conversation/${conversation.id}/stop`, cookies[1], {})).status).toBe(404);
    expect(server.concept.conversations.get(conversation.id).mode).toBe('complete');
    await request('/admin/concepts', null, { accountId: 'first', expired: true, models: ['First Only'] });
    expect(await (await request('/api/auth/session', cookies[0])).json()).toEqual({});
    expect((await (await request('/api/auth/session', cookies[1])).json()).user).toEqual(identities[1]);
    expect((await request('/api/conversation', cookies[0], { prompt: 'No expired submission' })).status).toBe(401);
    expect((await request('/api/conversation', cookies[1], { prompt: 'Other account remains ready' })).status).toBe(201);
    expect(server.concept.submissions.map(entry => entry.accountId)).toEqual(['first', 'second']);
    await request('/admin/concepts', null, { accountId: 'first', expired: false });
    expect((await (await request('/api/auth/session', cookies[0])).json()).models).toEqual(['First Only']);
    expect((await (await request('/api/auth/session', cookies[1])).json()).models).toEqual(['Fixture One', 'Fixture Two']);
  } finally { await server.stop(); }
});

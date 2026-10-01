import { randomUUID } from 'node:crypto';
import { readFile } from 'node:fs/promises';

const page = new URL('./concept.html', import.meta.url);
const client = new URL('./concept-client.mjs', import.meta.url);
const identity = { id: 'synthetic-chatgpt-account', email: 'synthetic@example.invalid' };

export function createConceptFixture() {
  const conversations = new Map();
  const submissions = [];
  const sessions = new Map();
  const accountControls = new Map();
  let pendingAuth = 0;
  const controls = { expired: false, account: identity, models: ['Fixture One', 'Fixture Two'], generationMs: 400, completionDelayMs: 300, mode: 'complete' };
  const send = (res, status, data) => {
    res.writeHead(status, { 'content-type': 'application/json', 'cache-control': 'no-store' });
    res.end(JSON.stringify(data));
  };
  const body = async (req) => {
    const chunks = [];
    let length = 0;
    for await (const chunk of req) {
      length += chunk.length;
      if (length > 1024 * 1024) throw new Error('fixture request too large');
      chunks.push(chunk);
    }
    return JSON.parse(Buffer.concat(chunks).toString() || '{}');
  };
  return {
    conversations, submissions, controls,
    async handle(req, res) {
      const url = new URL(req.url, 'http://localhost');
      const sessionId = /(?:^|;\s*)jailgun_synthetic_session=([^;]+)/.exec(req.headers.cookie || '')?.[1];
      const account = controls.isolatedAccounts ? sessions.get(sessionId) : controls.account;
      const scoped = controls.isolatedAccounts && account ? accountControls.get(account.id) ?? {} : controls;
      const authenticated = Boolean(controls.isolatedAccounts ? account : sessionId === 'synthetic') && !scoped.expired;
      if (req.method === 'GET' && (url.pathname === '/' || /^\/c\/[^/]+$/.test(url.pathname))) {
        res.writeHead(200, { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' });
        res.end(await readFile(page));
      } else if (req.method === 'GET' && url.pathname === '/concept-client.mjs') {
        res.writeHead(200, { 'content-type': 'text/javascript' });res.end(await readFile(client));
      } else if (req.method === 'POST' && url.pathname === '/api/test-login') {
        const sessionId = controls.isolatedAccounts ? randomUUID() : 'synthetic';
        if (controls.isolatedAccounts) {
          sessions.set(sessionId, structuredClone(controls.account));
          accountControls.set(controls.account.id, { ...accountControls.get(controls.account.id), expired: false });
        } else controls.expired = false;
        res.setHeader('set-cookie', `jailgun_synthetic_session=${sessionId}; HttpOnly; SameSite=Strict; Path=/; Max-Age=86400`);
        send(res, 200, { user: controls.account });
      } else if (req.method === 'GET' && url.pathname === '/api/auth/session') {
        const delay = scoped.nextAuthDelayMs ?? 0;
        scoped.nextAuthDelayMs = 0;
        if (delay) {
          pendingAuth++;
          await new Promise(resolve => setTimeout(resolve, delay));
          pendingAuth--;
        }
        send(res, 200, authenticated ? { user: account, models: scoped.models ?? controls.models } : {});
      } else if (req.method === 'POST' && url.pathname === '/api/conversation') {
        if (!authenticated) return send(res, 401, { error: 'authentication-expired' });
        const input = await body(req);
        if (typeof input.prompt !== 'string' || !input.prompt.trim()) return send(res, 400, { error: 'empty prompt' });
        const generation = { ...controls, ...scoped };
        submissions.push({ at: Date.now(), mode: generation.mode, accountId: account.id });
        if (generation.mode === 'rate-limit-uncertain') return send(res, 429, { error: 'account-rate-limited', retryAt: generation.retryAt });
        const id = randomUUID();
        const response = fixtureResponse(input.prompt);
        const entry = { id, userId: randomUUID(), assistantId: randomUUID(), prompt: input.prompt, model: input.model, createdAt: Date.now(), ...generation, accountId: account.id, response };
        conversations.set(id, entry);
        send(res, 201, { id });
      } else if (req.method === 'GET' && /^\/api\/conversation\/[^/]+$/.test(url.pathname)) {
        const entry = conversations.get(url.pathname.split('/').at(-1));
        if (!entry) return send(res, 404, { error: 'conversation missing' });
        if (controls.isolatedAccounts && entry.accountId !== account?.id) return send(res, 404, { error: 'conversation missing' });
        const elapsed = Date.now() - entry.createdAt;
        const progress = entry.mode === 'empty' ? 0 : entry.mode === 'partial' ? 0.45 : entry.mode === 'stopped' ? entry.stoppedProgress : Math.min(1, elapsed / entry.generationMs);
        send(res, 200, {
          id: entry.id, userId: entry.userId, assistantId: entry.assistantId, prompt: entry.prompt, model: entry.model,
          blocks: entry.response, progress,
          generating: entry.mode === 'partial' || (entry.mode !== 'stopped' && elapsed < entry.generationMs),
          complete: ['complete', 'empty'].includes(entry.mode) && elapsed >= entry.generationMs + entry.completionDelayMs,
          rateLimited: entry.mode === 'rate-limit', retryAt: entry.retryAt ?? null, expired: Boolean(scoped.expired),
        });
      } else if (req.method === 'POST' && /^\/api\/conversation\/[^/]+\/stop$/.test(url.pathname)) {
        const entry = conversations.get(url.pathname.split('/').at(-2));
        if (controls.isolatedAccounts && (!authenticated || entry?.accountId !== account?.id)) return send(res, 404, { error: 'conversation missing' });
        if (entry) { entry.stoppedProgress = entry.mode === 'partial' ? 0.45 : Math.min(1, (Date.now() - entry.createdAt) / entry.generationMs);entry.mode = 'stopped'; }
        send(res, 200, { stopped: Boolean(entry) });
      } else if (req.method === 'POST' && url.pathname === '/admin/concepts') {
        const input = await body(req);
        if (input.accountId) accountControls.set(input.accountId, { ...accountControls.get(input.accountId), ...input });
        else if (input.id) Object.assign(conversations.get(input.id) ?? {}, input);
        else Object.assign(controls, input);
        send(res, 200, { ok: true });
      } else if (req.method === 'GET' && url.pathname === '/admin/concepts') {
        send(res, 200, { pendingAuth, conversations: [...conversations.values()].map(({ id, model, createdAt, mode }) => ({ id, model, createdAt, mode })) });
      } else send(res, 404, { error: 'unknown fixture route' });
    },
  };
}

function fixtureResponse(prompt) {
  const blocks = [
    { tag: 'h1', text: 'Synthetic concept response' },
    { tag: 'p', text: 'This deterministic response is fixture evidence, not a live provider result.' },
    { tag: 'li', text: 'Test the practical value with a small pilot.' },
    { tag: 'pre', language: 'rust', text: 'fn main() {\n    println!("synthetic");\n}' },
    { tag: 'a', href: 'https://example.invalid/evidence', text: 'Synthetic evidence link' },
  ];
  if (prompt.includes('candidate_summary')) {
    blocks.push({ tag: 'pre', language: 'json', text: JSON.stringify({ candidate_summary: { proposal: 'A community lending library', strengths: ['Shared practical value'], weaknesses: ['Maintenance costs'], assumptions: ['Demand needs testing'], next_steps: ['Run a small pilot'] } }, null, 2) });
  } else if (prompt.includes('evaluations:')) {
    const raw = prompt.split('INPUT_DATA_JSON\n')[1]?.split('\nEND_INPUT_DATA_JSON')[0];
    const data = raw ? JSON.parse(raw) : {};
    const evaluations = (data.previous_outputs?.candidates ?? []).map(({ candidate }) => ({ candidate,
      scores: data.criteria.map(({ name }) => ({ criterion: name, score: 80, rationale: 'A deterministic model-judgment fixture' })), disagreements: [], uncertainty: 'Synthetic evidence only' }));
    blocks.push({ tag: 'pre', language: 'json', text: JSON.stringify({ evaluations }, null, 2) });
  }
  return blocks;
}

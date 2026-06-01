import { readFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const moduleDir = dirname(fileURLToPath(import.meta.url));
const DEFAULT_FIXTURES_DIR = resolve(
  moduleDir,
  '..',
  '..',
  'browser-adapter',
  'test-fixtures',
  'chatgpt'
);

const STATE_FIXTURE = {
  idle: 'idle.html',
  composing: 'composing.html',
  uploading: 'uploading.html',
  uploaded: 'uploaded-archive.html',
  generating: 'generating.html',
  'tar-ready': 'tar-ready-single.html',
  done: 'done-no-tar.html',
};

const OVERLAY_FIXTURE = {
  rate_limit: 'rate-limit-modal.html',
  session_expired: 'session-expired-modal.html',
  stay_on_page: 'stay-on-page-modal.html',
  github_prompt_deny: 'github-tool-deny.html',
  github_prompt_read: 'github-tool-read.html',
};

function pickStateFixture(entry) {
  if (entry.state === 'tar-ready' && entry.tarTargetName) {
    return entry.tarTargetName.includes('jekko-fixes') || entry.tarTargetName.includes('multi')
      ? 'tar-ready-multi.html'
      : 'tar-ready-single.html';
  }
  return STATE_FIXTURE[entry.state] ?? STATE_FIXTURE.idle;
}

async function loadFixtureBody(fixturesDir, name) {
  const path = join(fixturesDir, name);
  const html = await readFile(path, 'utf8');
  const match = html.match(/<body[^>]*>([\s\S]*?)<\/body>/i);
  return match ? match[1] : html;
}

async function readJsonBody(req) {
  return new Promise((resolveBody, rejectBody) => {
    const chunks = [];
    req.on('data', (chunk) => chunks.push(chunk));
    req.on('end', () => {
      const raw = Buffer.concat(chunks).toString('utf8');
      if (!raw) {
        resolveBody({});
        return;
      }
      try {
        resolveBody(JSON.parse(raw));
      } catch (error) {
        rejectBody(error);
      }
    });
    req.on('error', rejectBody);
  });
}

function sendJson(res, status, payload) {
  res.statusCode = status;
  res.setHeader('content-type', 'application/json');
  res.end(JSON.stringify(payload));
}

function sendHtml(res, body, conversationId, entry) {
  res.statusCode = 200;
  res.setHeader('content-type', 'text/html; charset=utf-8');
  res.end(
    `<!doctype html><html lang="en"><head><meta charset="utf-8" />` +
      `<title>Fake ChatGPT — ${conversationId}</title>` +
      `<meta name="fake-chatgpt-state" content="${entry.state}" />` +
      `<meta name="fake-chatgpt-overlays" content="${Array.from(entry.overlays).join(',')}" />` +
      `</head><body>${body}</body></html>`
  );
}

export function makeRouteHandler({ registry, fixturesDir = DEFAULT_FIXTURES_DIR }) {
  return async function handle(req, res) {
    try {
      const url = new URL(req.url ?? '/', `http://${req.headers.host || 'localhost'}`);
      const method = req.method || 'GET';

      if (method === 'GET' && url.pathname === '/') {
        sendJson(res, 200, {
          service: 'fake-chatgpt',
          message: 'use GET /c/:id for conversation pages, POST /admin/* to drive state',
          admin: ['POST /admin/state', 'POST /admin/advance', 'POST /admin/reset', 'GET /admin/status'],
        });
        return;
      }

      const conversationMatch = url.pathname.match(/^\/c\/([^/]+)\/?$/);
      if (method === 'GET' && conversationMatch) {
        const id = conversationMatch[1];
        const entry = registry.read(id);
        const baseFixture = pickStateFixture(entry);
        const baseBody = await loadFixtureBody(fixturesDir, baseFixture);
        const overlayBodies = [];
        for (const overlay of entry.overlays) {
          const fixture = OVERLAY_FIXTURE[overlay];
          if (!fixture) continue;
          overlayBodies.push(await loadFixtureBody(fixturesDir, fixture));
        }
        sendHtml(res, baseBody + overlayBodies.join(''), id, entry);
        return;
      }

      if (method === 'GET' && url.pathname === '/admin/status') {
        sendJson(res, 200, { conversations: registry.list() });
        return;
      }

      if (method === 'POST' && url.pathname === '/admin/state') {
        const payload = await readJsonBody(req);
        const id = payload.conversation_id;
        if (!id) {
          sendJson(res, 400, { error: 'conversation_id required' });
          return;
        }
        const entry = registry.set(id, {
          state: payload.state,
          overlays: payload.overlays,
          tarTargetName: payload.tar_target_name,
        });
        sendJson(res, 200, {
          conversation_id: id,
          state: entry.state,
          overlays: Array.from(entry.overlays),
          tarTargetName: entry.tarTargetName,
        });
        return;
      }

      if (method === 'POST' && url.pathname === '/admin/advance') {
        const payload = await readJsonBody(req);
        const id = payload.conversation_id;
        if (!id) {
          sendJson(res, 400, { error: 'conversation_id required' });
          return;
        }
        const entry = registry.advance(id);
        sendJson(res, 200, {
          conversation_id: id,
          state: entry.state,
          overlays: Array.from(entry.overlays),
        });
        return;
      }

      if (method === 'POST' && url.pathname === '/admin/reset') {
        registry.reset();
        sendJson(res, 200, { ok: true });
        return;
      }

      sendJson(res, 404, { error: `unknown route ${method} ${url.pathname}` });
    } catch (error) {
      sendJson(res, 500, { error: error.message });
    }
  };
}

export const DEFAULTS = { DEFAULT_FIXTURES_DIR, STATE_FIXTURE, OVERLAY_FIXTURE };

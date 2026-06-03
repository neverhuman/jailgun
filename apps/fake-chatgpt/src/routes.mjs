import { readFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';

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

function localizeDownloadUrls(body) {
  return body.replaceAll('https://files.example.invalid/', '/downloads/');
}

function automationScript(conversationId) {
  return `<script>
(() => {
  const conversationId = ${JSON.stringify(conversationId)};
  const composer = document.querySelector('#prompt-textarea,[data-testid="composer-text-input"],textarea,[contenteditable="true"]');
  const form = composer ? composer.closest('form') : document.querySelector('form');
  const send = document.querySelector('button[data-testid="send-button"],button[aria-label*="Send"],[data-testid*="send"]');
  let inFlight = false;
  let uploadReady = true;
  const uploadInput = document.createElement('input');
  uploadInput.type = 'file';
  uploadInput.setAttribute('data-testid', 'file-upload-input');
  uploadInput.setAttribute('aria-label', 'Attach file');
  uploadInput.style.position = 'absolute';
  uploadInput.style.left = '-10000px';
  form?.prepend(uploadInput);
  const setUploadChip = (filename, state) => {
    let chip = document.querySelector('[data-testid="upload-chip"]');
    if (!chip) {
      chip = document.createElement('div');
      chip.setAttribute('data-testid', 'upload-chip');
      form?.insertBefore(chip, composer || send || null);
    }
    chip.setAttribute('aria-label', state + ' ' + filename);
    chip.textContent = filename + ' ' + state;
  };
  const readComposer = () => {
    if (!composer) return '';
    if ('value' in composer) return composer.value || '';
    return composer.textContent || '';
  };
  const writeComposer = (value) => {
    if (!composer) return;
    if ('value' in composer) composer.value = value;
    else composer.textContent = value;
  };
  const updateSend = () => {
    if (!send) return;
    send.disabled = readComposer().trim().length === 0 || inFlight || !uploadReady;
    send.setAttribute('aria-disabled', send.disabled ? 'true' : 'false');
  };
  const submit = async (event) => {
    event.preventDefault();
    if (inFlight || !uploadReady || !readComposer().trim()) return;
    inFlight = true;
    writeComposer('');
    updateSend();
    await fetch('/admin/state', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ conversation_id: conversationId, state: 'tar-ready' }),
    });
    window.location.assign('/c/' + encodeURIComponent(conversationId));
  };
  uploadInput.addEventListener('change', () => {
    const file = uploadInput.files?.[0];
    if (!file) return;
    uploadReady = false;
    setUploadChip(file.name, 'Uploading');
    updateSend();
    setTimeout(() => {
      uploadReady = true;
      setUploadChip(file.name, 'Attached');
      updateSend();
    }, 500);
  });
  composer?.addEventListener('input', updateSend);
  form?.addEventListener('submit', submit);
  send?.addEventListener('click', submit);
  updateSend();
})();
</script>`;
}

function sendHtml(res, body, conversationId, entry) {
  res.statusCode = 200;
  res.setHeader('content-type', 'text/html; charset=utf-8');
  res.end(
    `<!doctype html><html lang="en"><head><meta charset="utf-8" />` +
      `<title>Fake ChatGPT — ${conversationId}</title>` +
      `<meta name="fake-chatgpt-state" content="${entry.state}" />` +
      `<meta name="fake-chatgpt-overlays" content="${Array.from(entry.overlays).join(',')}" />` +
      `</head><body>${localizeDownloadUrls(body)}${automationScript(conversationId)}</body></html>`
  );
}

export function makeRouteHandler({ registry, fixturesDir = DEFAULT_FIXTURES_DIR }) {
  let autoConversationCounter = 0;

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

      if (method === 'GET' && url.pathname === '/new') {
        autoConversationCounter += 1;
        const id = `auto-${Date.now()}-${autoConversationCounter}`;
        const entry = registry.read(id);
        const body = await loadFixtureBody(fixturesDir, STATE_FIXTURE.idle);
        sendHtml(res, body, id, entry);
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

      const downloadMatch = url.pathname.match(/^\/downloads\/([^/]+)$/);
      if (method === 'GET' && downloadMatch) {
        const filename = safeTarFilename(decodeURIComponent(downloadMatch[1]));
        const body = buildTarGz(filename);
        res.statusCode = 200;
        res.setHeader('content-type', 'application/gzip');
        res.setHeader('content-disposition', `attachment; filename="${filename}"`);
        res.end(body);
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

function safeTarFilename(value) {
  const name = String(value || 'chatgpt-output.tar.gz')
    .replace(/[/\\]/g, '-')
    .replace(/[^A-Za-z0-9_.-]/g, '-')
    .replace(/^-+|-+$/g, '');
  return name.endsWith('.tar.gz') ? name : `${name || 'chatgpt-output'}.tar.gz`;
}

function buildTarGz(filename) {
  const entryName = filename.replace(/\.tar\.gz$/i, '') + '/README.md';
  const content = Buffer.from(`# ${filename}\n\nGenerated by fake-chatgpt.\n`, 'utf8');
  const header = tarHeader(entryName, content.length);
  const padding = Buffer.alloc((512 - (content.length % 512)) % 512);
  return gzipSync(Buffer.concat([header, content, padding, Buffer.alloc(1024)]));
}

function tarHeader(name, size) {
  const header = Buffer.alloc(512, 0);
  header.write(name.slice(0, 100), 0, 100, 'utf8');
  writeOctal(header, 0o644, 100, 8);
  writeOctal(header, 0, 108, 8);
  writeOctal(header, 0, 116, 8);
  writeOctal(header, size, 124, 12);
  writeOctal(header, Math.floor(Date.now() / 1000), 136, 12);
  header.fill(0x20, 148, 156);
  header[156] = '0'.charCodeAt(0);
  header.write('ustar', 257, 5, 'ascii');
  header.write('00', 263, 2, 'ascii');
  const checksum = header.reduce((sum, byte) => sum + byte, 0);
  header.write(checksum.toString(8).padStart(6, '0'), 148, 6, 'ascii');
  header[154] = 0;
  header[155] = 0x20;
  return header;
}

function writeOctal(buffer, value, offset, length) {
  const octal = Number(value).toString(8).padStart(length - 1, '0').slice(-(length - 1));
  buffer.write(octal, offset, length - 1, 'ascii');
  buffer[offset + length - 1] = 0;
}

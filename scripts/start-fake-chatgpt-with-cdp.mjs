#!/usr/bin/env node
// Helper for the fake-chatgpt e2e CI lane.
//
// Spawns the fake-chatgpt HTTP server on a chosen port, waits for it to
// respond to `/admin/status`, prints `READY <url>` on stdout, then keeps
// the process alive until killed. The CI script captures the URL,
// drives the rest of the stack, and finally sends SIGTERM.
//
// The bridge itself launches headless Chromium against this URL — no
// Playwright shim is needed in this script. We rely on
// `apps/chrome-bridge/bin/chrome-bridge.mjs`'s existing
// `chromium.launchPersistentContext` path, just pointed at fake-chatgpt
// instead of chatgpt.com via the test config's `browser.chat_url`.

import { spawn } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(SCRIPT_DIR, '..');

const port = Number(process.env.FAKE_CHATGPT_PORT ?? 8082);
const url = `http://127.0.0.1:${port}`;

const child = spawn(
  'node',
  ['apps/fake-chatgpt/bin/fake-chatgpt.mjs', '--port', String(port)],
  {
    cwd: REPO_ROOT,
    stdio: ['ignore', 'inherit', 'inherit'],
    env: process.env,
  },
);

let exited = false;
child.on('exit', (code, signal) => {
  exited = true;
  process.exitCode = code ?? (signal ? 1 : 0);
  process.exit();
});

const deadline = Date.now() + 30_000;
while (Date.now() < deadline) {
  if (exited) {
    process.exit(process.exitCode ?? 1);
  }
  try {
    const response = await fetch(`${url}/admin/status`);
    if (response.ok) {
      const body = await response.json();
      if (body && typeof body === 'object') {
        console.log(`READY ${url}`);
        break;
      }
    }
  } catch {
    // server not up yet
  }
  await new Promise((r) => setTimeout(r, 200));
}

if (Date.now() >= deadline) {
  console.error('fake-chatgpt did not become ready within 30s');
  child.kill('SIGTERM');
  process.exit(1);
}

process.on('SIGINT', () => child.kill('SIGTERM'));
process.on('SIGTERM', () => child.kill('SIGTERM'));

// Park until killed. The child stays in the foreground via stdio:inherit
// so logs still surface in CI.
await new Promise(() => undefined);

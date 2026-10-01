#!/usr/bin/env node
// Exercise the production Rust server and built UI with a private synthetic installation.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import net from 'node:net';
import { resolve, join } from 'node:path';
import { chromium } from 'playwright-core';

await mkdir('target/auth-e2e', { recursive: true, mode: 0o700 });
const runtime = await mkdtemp(resolve('target/auth-e2e/installation-'));
const listener = net.createServer();
listener.listen(0, '127.0.0.1');
await once(listener, 'listening');
const port = listener.address().port;
await new Promise(done => listener.close(done));
const base = `http://127.0.0.1:${port}`;
const dashboard = `${base}/?advanced=1`;
const child = spawn(resolve('target/debug/jailgun'), ['serve', '--live', '--addr', `127.0.0.1:${port}`, '--dashboard-dist', resolve('apps/dashboard/dist')], {
  env: { PATH: process.env.PATH, HOME: runtime, JAILGUN_BROWSER_PROFILES: join(runtime, 'browser-profiles.json') },
  stdio: ['ignore', 'pipe', 'pipe'],
});
let output = '';
let errorOutput = '';
child.stdout.on('data', data => { output = (output + data).slice(-16_384); });
child.stderr.on('data', data => { errorOutput = (errorOutput + data).slice(-16_384); });
let context;
const result = { checked_at: new Date().toISOString(), status: 'fail', checks: [] };
try {
  let ready = false;
  for (let i = 0; i < 200; i++) {
    if (child.exitCode !== null) throw new Error(`server exited: ${errorOutput}`);
    if (await fetch(`${base}/api/health`).then(r => r.ok).catch(() => false)) { ready = true; break; }
    await new Promise(done => setTimeout(done, 50));
  }
  assert.ok(ready, 'server did not become ready');
  assert.equal((await fetch(`${base}/api/runs`)).status, 401);
  const token = (await readFile(join(runtime, 'operator-token'), 'utf8')).trim();
  const issuedPairing = await fetch(`${base}/api/session/pairing-code`, { method: 'POST', headers: { Authorization: `Bearer ${token}` } });
  assert.equal(issuedPairing.status, 200);
  const { code } = await issuedPairing.json();
  assert.ok(code);
  assert.ok(![output, errorOutput].some(log => log.includes(code) || log.includes(token) || /#pair=|[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}/i.test(log)), 'server logged a pairing credential');
  result.checks.push('explicit-pairing-without-startup-secret');
  context = await chromium.launchPersistentContext(join(runtime, 'test-browser'), { executablePath: process.env.JAILGUN_TEST_CHROME || undefined, headless: true, chromiumSandbox: true });
  const page = await context.newPage();
  page.setDefaultTimeout(10_000);
  await page.goto(dashboard);
  await page.getByLabel('Pairing code').fill(code);
  await page.getByRole('button', { name: 'Pair browser' }).click();
  await page.getByRole('heading', { name: 'No runs yet' }).waitFor();
  const cookies = await context.cookies();
  assert.equal(cookies.length, 1);
  assert.equal(cookies[0].httpOnly, true);
  assert.equal(cookies[0].sameSite, 'Strict');
  assert.equal(await page.evaluate(() => localStorage.length), 0);
  result.checks.push('operator-cookie-pairing');
  await page.reload();
  await page.getByRole('heading', { name: 'No runs yet' }).waitFor();
  result.checks.push('session-reuse');
  const replay = await page.evaluate(async code => (await fetch('/api/session/pair', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ code }) })).status, code);
  assert.equal(replay, 401);
  result.checks.push('single-use-code');
  const websocket = await page.evaluate(() => new Promise(resolveResult => {
    const socket = new WebSocket(`ws://${location.host}/ws/events`);
    const timer = setTimeout(() => { socket.close(); resolveResult(false); }, 3000);
    socket.onopen = () => { clearTimeout(timer); socket.close(); resolveResult(true); };
    socket.onerror = () => { clearTimeout(timer); resolveResult(false); };
  }));
  assert.ok(websocket, 'cookie WebSocket was rejected');
  result.checks.push('cookie-websocket');
  const issued = await page.evaluate(async () => {
    const response = await fetch('/api/session/pairing-code', { method: 'POST' });
    if (!response.ok) throw new Error('could not issue setup pairing code');
    return response.json();
  });
  await context.clearCookies();
  let exchanges = 0;
  page.on('request', request => { if (request.url().endsWith('/api/session/pair')) exchanges += 1; });
  await page.goto(`${dashboard}#pair=invalid`);
  await page.getByRole('alert').filter({ hasText: 'pairing link is invalid' }).waitFor();
  assert.equal(exchanges, 0);
  await page.goto(`${dashboard}#pair=${issued.code}&next=accounts`);
  await page.waitForURL(`${dashboard}#accounts`);
  await page.getByRole('heading', { name: 'No runs yet' }).waitFor();
  assert.equal(exchanges, 1);
  assert.equal(new URL(page.url()).hash, '#accounts');
  assert.equal(await page.evaluate(() => localStorage.length), 0);
  result.checks.push('setup-link-single-exchange-and-history-removal');
  result.status = 'pass';
} catch (error) {
  // Do not persist process output, pairing codes, or cookies.
  result.error = error.message.replace(/[a-f0-9]{8}-[a-f0-9-]{27,}/g, '[redacted]');
  process.exitCode = 1;
} finally {
  await context?.close();
  child.kill('SIGTERM');
  if (child.exitCode === null) await once(child, 'exit');
  await writeFile('target/auth-e2e/evidence.json', `${JSON.stringify(result, null, 2)}\n`);
  console.log(JSON.stringify(result));
}

#!/usr/bin/env node
// Executes the compiled Rust daemon; this proof owns no application state transitions.
import assert from 'node:assert/strict';
import { createHash, randomUUID } from 'node:crypto';
import { spawn, spawnSync, execFileSync } from 'node:child_process';
import { mkdir, open, readFile, stat, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const output = resolve(root, 'target/daemon proofs', randomUUID());
const runtime = resolve(output, 'private runtime');
const cwd = resolve(output, 'outside checkout');
await mkdir(cwd, { recursive: true, mode: 0o700 });
const listener = createServer();
await new Promise((done, fail) => { listener.once('error', fail); listener.listen(0, '127.0.0.1', done); });
const port = listener.address().port;
await new Promise((done) => listener.close(done));
const binary = resolve(root, 'target/debug/jailgun');
const args = ['serve', '--concepts', '--runtime', runtime, '--assets', root, '--node', process.execPath, '--addr', `127.0.0.1:${port}`];
const url = `http://127.0.0.1:${port}`;
const delay = (ms) => new Promise((done) => setTimeout(done, ms));
const get = (path, token) => fetch(url + path, { signal: AbortSignal.timeout(2000), headers: token ? { Authorization: `Bearer ${token}` } : {} });
let token;
for (let iteration = 0; iteration < 2; iteration += 1) {
  const log = await open(resolve(output, `daemon-${iteration}.log`), 'wx', 0o600);
  const child = spawn(binary, args, { cwd, stdio: ['ignore', log.fd, log.fd] });
  let stoppedViaCommand = false;
  const exit = new Promise((done) => { child.once('exit', (code, signal) => done({ code, signal })); child.once('error', () => done({ code: -1 })); });
  try {
    const deadline = Date.now() + 15000;
    for (;;) {
      assert.equal(child.exitCode, null, 'daemon exited before readiness; inspect private log');
      try { assert.deepEqual(await (await get('/api/health')).json(), { status: 'ok' }); break; }
      catch (error) { if (Date.now() > deadline) throw error; await delay(100); }
    }
    const credential = resolve(runtime, 'operator-token');
    const stored = (await readFile(credential, 'utf8')).trim();
    assert.equal(stored.length, 64);
    assert.equal((await stat(credential)).mode & 0o777, 0o600);
    if (token) assert.equal(stored, token);
    token = stored;
    const startupLog = await readFile(resolve(output, `daemon-${iteration}.log`), 'utf8');
    assert.ok(!startupLog.includes(token), 'daemon logged the operator credential');
    assert.ok(!/Pairing code|#pair=|[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}/i.test(startupLog), 'daemon logged a pairing credential');
    assert.equal((await get('/api/accounts')).status, 401);
    assert.deepEqual(await (await get('/api/accounts', token)).json(), []);
    const duplicate = spawnSync(binary, args, { cwd, timeout: 5000, encoding: 'utf8' });
    assert.notEqual(duplicate.status, 0);
    assert.match(duplicate.stderr, /daemon-already-owned/);
    if (iteration === 1) {
      const status = JSON.parse(execFileSync(binary, ['service', 'status', '--runtime', runtime, '--json'], { cwd, timeout: 10_000, encoding: 'utf8' }));
      assert.equal(status.state, 'running');
      const stopped = JSON.parse(execFileSync(binary, ['service', 'stop', '--runtime', runtime, '--json'], { cwd, timeout: 40_000, encoding: 'utf8' }));
      assert.equal(stopped.state, 'stopped');
      assert.equal(stopped.instance_id, status.instance_id);
      stoppedViaCommand = true;
    }
  } finally {
    if (!stoppedViaCommand) child.kill('SIGTERM');
    let timer;
    const stopped = await Promise.race([exit, new Promise((done) => { timer = setTimeout(() => done(null), 8000); })]);
    clearTimeout(timer);
    if (!stopped) { child.kill('SIGKILL'); await exit; }
    await log.close();
    assert.ok(stopped, 'SIGTERM did not shut down the daemon');
    assert.equal(stopped.code, 0);
  }
}
const evidence = {
  status: 'pass', exit_status: 0, generated_at: new Date().toISOString(),
  checks: ['outside-checkout', 'spaces-in-paths', 'private-credentials', 'no-startup-pairing-secret', 'authenticated-http', 'duplicate-owner-rejected', 'sigterm-shutdown', 'authenticated-instance-shutdown', 'credential-and-database-restart'],
  binary_sha256: createHash('sha256').update(await readFile(binary)).digest('hex'),
  proof_sha256: createHash('sha256').update(await readFile(fileURLToPath(import.meta.url))).digest('hex'),
  commit: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(),
  dirty: Boolean(execFileSync('git', ['status', '--porcelain'], { cwd: root, encoding: 'utf8' }).trim()),
  node: process.version, installed_bundle_verified: false,
};
await writeFile(resolve(output, 'evidence.json'), JSON.stringify(evidence, null, 2) + '\n', { mode: 0o600 });
console.log('pass: nine concept daemon checks; private evidence', resolve(output, 'evidence.json'));

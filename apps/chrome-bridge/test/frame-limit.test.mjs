import assert from 'node:assert/strict';
import test from 'node:test';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { mkdir, mkdtemp } from 'node:fs/promises';
import { join } from 'node:path';
import { readFrames, MAX_LINE_BYTES } from '../src/bridge-frames.mjs';

async function* chunks(...values) { yield* values; }

async function within(promise, milliseconds, message) {
  let timer;
  try {
    return await Promise.race([promise, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(message)), milliseconds);
    })]);
  } finally { clearTimeout(timer); }
}

test('frame boundaries preserve exact limits, CRLF, split UTF-8 and final EOF', async () => {
  const utf8 = Buffer.from('café');
  const frames = [];
  for await (const frame of readFrames(chunks(Buffer.alloc(MAX_LINE_BYTES, 'x'), Buffer.from('\r'), Buffer.from('\n'), utf8.subarray(0, 4), utf8.subarray(4), Buffer.from('\nlast')))) frames.push(frame);
  assert.equal(frames[0].length, MAX_LINE_BYTES);
  assert.deepEqual(frames.slice(1), ['café', 'last']);
});

test('invalid UTF-8 and over-limit payloads fail without exposing contents', async () => {
  for (const [bytes, code] of [[Buffer.from([0xff, 10]), 'bridge-frame-invalid-utf8'], [Buffer.alloc(MAX_LINE_BYTES + 1, 'x'), 'bridge-frame-too-large']]) {
    await assert.rejects(async () => { for await (const _ of readFrames(chunks(bytes))) assert.fail('invalid frame emitted'); }, { message: code });
  }
});

test('an oversized open input frame fails before newline or EOF', async () => {
  // A slow cold start must not consume the deadline for rejecting an input frame.
  const child = spawn(process.execPath, ['--import', 'data:text/javascript,await new Promise(resolve => setTimeout(resolve, 1750));', fileURLToPath(new URL('../bin/concept-bridge.mjs', import.meta.url))], { env: { PATH: process.env.PATH }, stdio: ['pipe', 'pipe', 'pipe'] });
  let diagnostic = ''; child.stderr.on('data', bytes => { diagnostic += bytes; });
  child.stdin.on('error', () => {});
  const lines = createInterface({ input: child.stdout });
  const closed = once(child, 'close');
  try {
    const ready = once(lines, 'line');
    child.stdin.write(JSON.stringify({ v: 1, id: 'readiness', type: 'concept.auth-status', run_id: 'framing', payload: {} }) + '\n');
    const [line] = await within(ready, 10_000, 'bridge startup handshake timed out');
    const response = JSON.parse(line);
    assert.equal(response.correlation_id, 'readiness');
    assert.equal(response.payload.code, 'browser-unavailable');
    // Keep stdin open: rejection must not depend on a newline, EOF, or browser startup.
    child.stdin.write(Buffer.alloc(MAX_LINE_BYTES + 1, 'x'));
    const result = await within(closed, 1500, 'oversized input stayed open');
    assert.notEqual(result[0], 0);
    assert.match(diagnostic, /bridge-frame-too-large/);
    assert.ok(diagnostic.length < 200, 'input contents must not enter diagnostics');
  } finally {
    lines.close();
    if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
    await closed;
  }
});

test('EOF during initialization prevents a later browser launch', async () => {
  const root = fileURLToPath(new URL('../../../target/bridge-framing/', import.meta.url));
  await mkdir(root, { recursive: true, mode: 0o700 });
  const profile = await mkdtemp(join(root, 'eof-'));
  const child = spawn(process.execPath, [fileURLToPath(new URL('../bin/concept-bridge.mjs', import.meta.url))], { env: { PATH: process.env.PATH }, stdio: ['pipe', 'pipe', 'pipe'] });
  let output = '';child.stdout.on('data', bytes => { output += bytes; });
  child.stdin.on('error', () => {});
  const exited = once(child, 'exit');
  // A launch attempt would report browser-operation-failed for this absent executable.
  child.stdin.end(JSON.stringify({ v: 1, id: 'initialize', type: 'concept.initialize', run_id: 'account', payload: { profile_dir: profile, executable: join(profile, 'absent-browser'), headless: true } }) + '\n');
  let timer;
  try {
    const result = await Promise.race([exited, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('initialization survived EOF')), 3000); })]);
    assert.equal(result[0], 0);
    assert.equal(JSON.parse(output).payload.code, 'browser-unavailable');
  } finally { clearTimeout(timer);if (child.exitCode === null) child.kill('SIGKILL');await exited; }
});

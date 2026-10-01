#!/usr/bin/env node
import { mkdir, chmod, lstat } from 'node:fs/promises';
import { isAbsolute, join } from 'node:path';
import { chromium } from 'playwright-core';
import { AdapterError, ChatGPTProvider } from '../src/chatgpt-provider.mjs';
import { selectAccountPage } from '../src/account-page.mjs';
import { MAX_LINE_BYTES, readFrames } from '../src/bridge-frames.mjs';
import { clearStaleProfileLocks, startManagedChrome, stopManagedChrome } from '../src/managed-chrome.mjs';

process.umask(0o077);
const owned = new Map();
let context;
let browser;
let browserProcess;
let options;
let starting;
let closing = false;
let accountPage;

function respond(request, payload, failed = false) {
  const response = { v: 1, type: failed ? 'concept.error' : 'concept.result', correlation_id: request.id, run_id: request.run_id, ts: new Date().toISOString(), payload };
  const line = JSON.stringify(response);
  if (Buffer.byteLength(line) > MAX_LINE_BYTES) {
    process.stdout.write(JSON.stringify({ ...response, type: 'concept.error', payload: { code: 'capture-too-large', message: 'The response exceeds the bridge frame limit; the owned page is retained.', next_action: 'Inspect and export the owned conversation before retrying.' } }) + '\n');
  } else process.stdout.write(line + '\n');
}

function own(request) {
  const attempt = request.payload.attempt_id;
  const entry = owned.get(attempt);
  if (!entry || entry.runId !== request.run_id) throw new AdapterError('ownership-mismatch', 'The request does not own this browser conversation.');
  return entry;
}

async function initialize(input) {
  if (context || starting) throw new AdapterError('browser-already-started', 'This supervisor already manages a browser.');
  starting = launch(input);
  try { return await starting; } finally { starting = null; }
}

async function launch(input) {
  if (!isAbsolute(input.profile_dir)) throw new AdapterError('profile-path-invalid', 'The managed profile path must be absolute.');
  clearStaleProfileLocks(input.profile_dir);
  for (const name of ['SingletonLock', 'SingletonCookie', 'SingletonSocket']) {
    try { await lstat(join(input.profile_dir, name));throw new AdapterError('profile-locked', 'The profile has an existing browser ownership lock.', 'Stop or reconnect the verified owning browser; profile locks are preserved.'); }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
  }
  // Validate provider configuration before launching any process.
  new ChatGPTProvider(null, { identity: input.identity, baseUrl: input.base_url });
  options = input;
  await mkdir(input.profile_dir, { recursive: true, mode: 0o700 });
  await chmod(input.profile_dir, 0o700);
  if (closing) throw new AdapterError('browser-unavailable', 'The account browser is closing.');
  const base = new URL(input.base_url || 'https://chatgpt.com');
  if (input.headless === true && !['127.0.0.1', 'localhost'].includes(base.hostname)) {
    throw new AdapterError('browser-display-required', 'ChatGPT accounts require a normal interactive Chrome display.');
  }
  const managed = await startManagedChrome({
    executable: input.executable,
    profileDir: input.profile_dir,
    port: Number(input.cdp_port),
    headless: input.headless === true,
  });
  browserProcess = managed.child;
  try {
    browser = await chromium.connectOverCDP(managed.endpoint, { timeout: 30_000 });
  } catch (error) {
    await stopManagedChrome(browserProcess);
    browserProcess = null;
    throw error;
  }
  context = browser.contexts()[0];
  if (!context) throw new AdapterError('browser-context-missing', 'Managed Chrome did not expose its default context.');
  if (closing) throw new AdapterError('browser-unavailable', 'The account browser is closing.');
  accountPage = await selectAccountPage(context, accountPage, input.base_url);
  await accountPage.goto(input.base_url || 'https://chatgpt.com', { waitUntil: 'domcontentloaded' });
  return { started: true, browser_version: await browser.version(), sandbox: true };
}

async function handle(request) {
  const payload = request.payload || {};
  if (closing) throw new AdapterError('browser-unavailable', 'The account browser is closing.');
  if (request.type === 'concept.initialize') return initialize(payload);
  if (!context || closing) throw new AdapterError('browser-unavailable', 'The account browser is not running.');
  if (request.type === 'concept.auth-status') {
    const page = await selectAccountPage(context, accountPage, options.base_url);
    accountPage = page;
    const provider = new ChatGPTProvider(page, { identity: options.identity, baseUrl: options.base_url });
    const { identity, model } = await provider.checkReadiness();
    const available_models = payload.discover_models === true ? await provider.availableModels() : [model];
    return { identity, model, available_models };
  }
  if (request.type === 'concept.bind-identity') {
    accountPage = await selectAccountPage(context, accountPage, options.base_url);
    const provider = new ChatGPTProvider(accountPage, { identity: payload.identity, baseUrl: options.base_url });
    options.identity = await provider.verifyIdentity();
    return { bound: true };
  }
  if (request.type === 'concept.login-open') {
    accountPage = await selectAccountPage(context, accountPage, options.base_url);
    await accountPage.goto(options.base_url || 'https://chatgpt.com', { waitUntil: 'domcontentloaded' });
    await accountPage.bringToFront();
    return { opened: true };
  }
  if (request.type === 'concept.fixture-login') {
    if (!['127.0.0.1', 'localhost'].includes(new URL(options.base_url).hostname)) throw new AdapterError('fixture-operation-denied', 'Synthetic login is restricted to the loopback fixture.');
    const page = context.pages()[0];
    await page.getByRole('button', { name: 'Sign in to synthetic account', exact: true }).click();
    await page.getByRole('button', { name: 'Sign in to synthetic account', exact: true }).waitFor({ state: 'hidden' });
    return { complete: true };
  }
  if (request.type === 'concept.prepare' || request.type === 'concept.reattach') {
    if (!payload.attempt_id || owned.has(payload.attempt_id)) throw new AdapterError('attempt-already-owned', 'This attempt already owns a page.');
    if (owned.size >= 10) throw new AdapterError('browser-capacity', 'The managed browser already has ten owned conversations.');
    const page = await context.newPage();
    const provider = new ChatGPTProvider(page, {
      identity: options.identity,
      baseUrl: options.base_url,
      model: payload.model,
      reasoningEffort: payload.reasoning_effort,
      attachmentPath: payload.attachment_path,
      timeoutMs: payload.timeout_ms,
    });
    const entry = { page, provider, runId: request.run_id, phase: 'preparing', abort: new AbortController() };
    owned.set(payload.attempt_id, entry);
    if (request.type === 'concept.reattach') {
      const origin = new URL(options.base_url || 'https://chatgpt.com').origin;
      const turn = payload.turn;
      if (!turn?.user_turn_id || new URL(turn.conversation_url).origin !== origin) throw new AdapterError('reconciliation-required', 'An owned conversation and accepted user turn are required for reattachment.');
      await page.goto(turn.conversation_url, { waitUntil: 'domcontentloaded' });await provider.verifyIdentity();
      entry.turn = turn;provider.activeTurn = turn;entry.phase = 'accepted';return { reattached: true };
    }
    const prepared = await provider.prepare(payload.prompt);
    entry.phase = 'prepared';return prepared;
  }
  if (request.type === 'concept.submit') {
    const entry = own(request);
    if (entry.phase !== 'prepared') throw new AdapterError('submission-uncertain', 'The owned page is not awaiting a first submission.');
    entry.phase = 'submitting';
    entry.submission = entry.provider.submit();
    entry.turn = await entry.submission;entry.phase = 'accepted';return entry.turn;
  }
  if (request.type === 'concept.capture') {
    const entry = own(request);
    if (entry.phase !== 'accepted') throw new AdapterError('capture-state-invalid', 'Capture requires a known accepted prompt.');
    entry.phase = 'capturing';
    const result = await entry.provider.capture(entry.turn, { signal: entry.abort.signal });
    entry.phase = 'captured';return result;
  }
  if (request.type === 'concept.stop') {
    const entry = own(request);
    entry.stopping ??= (async () => {
      const reason = ['run-deadline', 'response-timeout'].includes(payload.reason) ? payload.reason : 'cancelled';
      entry.abort.abort(new AdapterError(reason, 'The supervisor stopped this owned conversation.'));
      if (entry.submission) await entry.submission;
      await entry.provider.stop();
      return { stopped: true };
    })();
    try { return await entry.stopping; }
    catch (error) { entry.stopping = null; throw error; }
  }
  if (request.type === 'concept.close') {
    const entry = own(request);
    if (payload.durable_capture !== true && payload.confirmed_not_submitted !== true) throw new AdapterError('capture-not-durable', 'The page must be retained until capture is durable or non-submission is confirmed.');
    if (entry.phase === 'capturing' || entry.phase === 'submitting') throw new AdapterError('conversation-active', 'An active conversation cannot be closed.');
    await entry.page.close();owned.delete(payload.attempt_id);return { closed: true };
  }
  if (request.type === 'concept.shutdown') {
    await shutdown();return { stopped: true };
  }
  throw new AdapterError('bridge-command-unknown', 'Unknown concept browser operation.');
}

async function shutdown() {
  closing = true;
  for (const entry of owned.values()) entry.abort.abort();
  // EOF may arrive while launch is awaiting filesystem or browser startup.
  await starting?.catch(() => {});
  await browser?.close().catch(() => {});browser = null;context = null;
  await stopManagedChrome(browserProcess);browserProcess = null;
}

function dispatch(line) {
  let request;
  try { request = JSON.parse(line); }
  catch { throw new Error('bridge-frame-invalid-json'); }
  if (request?.v !== 1 || typeof request.id !== 'string' || !request.id || typeof request.run_id !== 'string') throw new Error('bridge-envelope-invalid');
  // Captures must run concurrently so other owned conversations remain usable.
  handle(request).then((payload) => respond(request, payload)).catch((error) => {
    respond(request, { code: error.code || 'browser-operation-failed', message: error instanceof AdapterError ? error.message : 'The browser operation failed; owned pages are retained.', next_action: error.nextAction || 'Inspect the managed browser and local prerequisites.', retry_at_ms: error.retryAtMs ?? null, rate_limited: error.rateLimited === true }, true);
  });
}

try { for await (const line of readFrames(process.stdin)) dispatch(line); }
catch (error) {
  process.stderr.write(`${['bridge-frame-too-large', 'bridge-frame-invalid-utf8', 'bridge-frame-invalid-json', 'bridge-envelope-invalid'].includes(error.message) ? error.message : 'bridge-input-failed'}\n`);
  process.exitCode = 1;
} finally { await shutdown(); }

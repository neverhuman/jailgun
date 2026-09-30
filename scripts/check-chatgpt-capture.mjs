import assert from 'node:assert/strict';
import { createHash, randomUUID } from 'node:crypto';
import { existsSync } from 'node:fs';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { execFileSync } from 'node:child_process';
import { chromium } from 'playwright-core';
import { start } from '../apps/fake-chatgpt/src/server.mjs';
import { ChatGPTProvider } from '../apps/chrome-bridge/src/chatgpt-provider.mjs';

const output = resolve('target/browser-capture', randomUUID());
await mkdir(output, { recursive: true, mode: 0o700 });
const service = await start({ port: 0, interactive: true });
const identity = { id: 'synthetic-chatgpt-account', email: 'synthetic@example.invalid' };
const executablePath = process.env.JAILGUN_TEST_CHROME || (existsSync('/usr/bin/google-chrome') ? '/usr/bin/google-chrome' : chromium.executablePath());
const profile = resolve(output, 'profile');
let context;
const checks = [];
const consoleErrors = [];
const check = async (name, action) => {
  const started = Date.now();
  try { await action();checks.push({ name, status: 'pass', duration_ms: Date.now() - started }); }
  catch (error) { checks.push({ name, status: 'fail', error: error.message });throw error; }
};
const launch = async () => {
  context = await chromium.launchPersistentContext(profile, { executablePath, headless: true, chromiumSandbox: true });
  const page = context.pages()[0] || await context.newPage();
  page.on('pageerror', (error) => consoleErrors.push(error.message));
  return page;
};
const providerFor = (page, options = {}) => new ChatGPTProvider(page, { identity, baseUrl: service.url, pollMs: 30, settleMs: 150, timeoutMs: 3000, ...options });
let page;
let error;
try {
  page = await launch();
  await check('anonymous composer does not authenticate', async () => {
    await assert.rejects(providerFor(page).prepare('Synthetic concept'), { code: 'authentication-expired' });
    assert.equal(service.concept.conversations.size, 0);
  });
  await check('interactive synthetic login and identity binding', async () => {
    await page.getByRole('button', { name: 'Sign in to synthetic account' }).click();
    await page.getByRole('button', { name: 'Sign in to synthetic account' }).waitFor({ state: 'hidden' });
    assert.deepEqual(await providerFor(page).verifyIdentity(), identity);
    await assert.rejects(providerFor(page, { identity: { ...identity, id: 'wrong-account' } }).verifyIdentity(), { code: 'account-mismatch' });
  });
  await check('model discovery selection and unavailable model', async () => {
    const provider = providerFor(page, { model: { mode: 'specific', name: 'Fixture Two' } });
    assert.deepEqual(await provider.availableModels(), ['Fixture One', 'Fixture Two']);
    assert.equal((await provider.prepare('Synthetic concept')).observed_model, 'Fixture Two');
    await assert.rejects(providerFor(page, { model: { mode: 'specific', name: 'Unavailable Model' } }).prepare('Synthetic concept'), { code: 'model-unavailable' });
    assert.equal(service.concept.conversations.size, 0);
  });
  await check('streaming settled completion preserves Markdown code and links', async () => {
    const provider = providerFor(page);
    await provider.prepare('Synthetic concept with candidate_summary');
    const turn = await provider.submit();
    const started = Date.now();
    const result = await provider.capture(turn);
    assert.equal(result.complete, true, result.error_code);
    assert.ok(Date.now() - started >= 150);
    assert.match(result.markdown, /^# Synthetic concept response/);
    assert.match(result.markdown, /- Test the practical value/);
    assert.match(result.markdown, /```rust\nfn main\(\)/);
    assert.match(result.markdown, /\[Synthetic evidence link\]\(https:\/\/example.invalid\/evidence\)/);
    assert.match(result.markdown, /```json\n/);
    assert.equal(result.observed_model, 'Fixture Two');
    await assert.rejects(provider.submit(), { code: 'adapter-not-prepared' });
  });
  await check('persistent profile reuses login after browser restart', async () => {
    await context.close();page = await launch();await page.goto(service.url);
    assert.deepEqual(await providerFor(page).verifyIdentity(), identity);
    assert.equal(await providerFor(page).currentModel(), 'Fixture Two');
  });
  await check('incomplete stream remains partial with original timeout', async () => {
    service.concept.controls.mode = 'partial';
    const provider = providerFor(page, { timeoutMs: 400 });
    await provider.prepare('Synthetic interrupted concept');const turn = await provider.submit();
    const result = await provider.capture(turn);
    assert.equal(result.complete, false);assert.equal(result.error_code, 'response-timeout');assert.ok(result.markdown.length > 0);
    await provider.stop();
    assert.equal(service.concept.conversations.get(turn.conversation_id).mode, 'stopped');
  });
  await check('natural completion during an unclickable Stop is confirmed from the owned turn', async () => {
    service.concept.controls.mode = 'complete';
    service.concept.controls.generationMs = 800;
    const provider = providerFor(page);
    await provider.prepare('Synthetic cancellation completion race');const turn = await provider.submit();
    await page.evaluate(() => {
      const overlay = document.createElement('div');overlay.id = 'synthetic-click-obstruction';
      overlay.style.cssText = 'position:fixed;inset:0;z-index:9999';document.body.append(overlay);
    });
    try {
      await provider.stop();
      assert.equal(service.concept.conversations.get(turn.conversation_id).mode, 'complete');
      assert.equal((await provider.capture(turn)).complete, true);
    } finally {
      await page.locator('#synthetic-click-obstruction').evaluate(element => element.remove());
      service.concept.controls.generationMs = 400;
    }
  });
  await check('empty completed response is rejected', async () => {
    service.concept.controls.mode = 'empty';
    const provider = providerFor(page);await provider.prepare('Synthetic empty response');const result = await provider.capture(await provider.submit());
    assert.equal(result.complete, false);assert.equal(result.error_code, 'empty-response');
  });
  await check('session expiry preserves partial output', async () => {
    service.concept.controls.mode = 'partial';
    const provider = providerFor(page);await provider.prepare('Synthetic expiry');const turn = await provider.submit();
    service.concept.controls.expired = true;
    const result = await provider.capture(turn);
    assert.equal(result.complete, false);assert.equal(result.error_code, 'authentication-expired');
    service.concept.controls.expired = false;await provider.stop();
  });
  await check('rate limit is not dismissed into success', async () => {
    service.concept.controls.mode = 'complete';
    const provider = providerFor(page);await provider.prepare('Synthetic limit');const turn = await provider.submit();
    service.concept.conversations.get(turn.conversation_id).mode = 'rate-limit';
    const retryAt = new Date(Date.now() + 600_000).toISOString();
    service.concept.conversations.get(turn.conversation_id).retryAt = retryAt;
    const result = await provider.capture(turn);
    assert.equal(result.complete, false);assert.equal(result.error_code, 'account-rate-limited');
    assert.equal(result.retry_at_ms, Date.parse(retryAt));
    await assert.rejects(provider.checkReadiness(), { code: 'account-rate-limited', rateLimited: true, retryAtMs: Date.parse(retryAt) });
    service.concept.conversations.get(turn.conversation_id).retryAt = 'unparseable provider text';
    await page.locator('time[datetime="unparseable provider text"]').waitFor();
    await assert.rejects(provider.checkReadiness(), { code: 'account-rate-limited', retryAtMs: null });
  });
  await check('different user turn cannot be salvaged as requested answer', async () => {
    const provider = providerFor(page);await provider.prepare('Synthetic turn ownership');const turn = await provider.submit();
    const result = await provider.capture({ ...turn, user_turn_id: 'unrelated-turn' });
    assert.equal(result.complete, false);assert.equal(result.error_code, 'adapter-turn-missing');assert.equal(result.markdown, '');
  });
  await check('browser page scripts complete without errors', async () => { assert.deepEqual(consoleErrors, []); });
} catch (caught) { error = caught; }
finally { await context?.close();await service.stop(); }
const inputs = ['apps/browser-adapter/src/domContracts/chatgptText.mjs','apps/chrome-bridge/src/chatgpt-provider.mjs','apps/chrome-bridge/src/auth-signal.mjs','apps/fake-chatgpt/src/concept.mjs','apps/fake-chatgpt/src/concept-client.mjs','apps/fake-chatgpt/src/concept.html','scripts/check-chatgpt-capture.mjs'];
const hashes = {};
for (const path of inputs) hashes[path] = createHash('sha256').update(await readFile(path)).digest('hex');
const evidence = { generated_at: new Date().toISOString(), scope: 'real-chrome-synthetic-provider-adapter', status: error ? 'fail' : 'pass', live_provider_verified: false,
  commit: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(), dirty: Boolean(execFileSync('git', ['status', '--porcelain'], { encoding: 'utf8' }).trim()),
  node: process.version, chromium_sandbox: true, input_sha256: hashes, checks };
await writeFile(resolve(output, 'evidence.json'), JSON.stringify(evidence, null, 2) + '\n', { mode: 0o600 });
console.log(`${evidence.status}: ${checks.length} real-browser adapter checks; evidence ${output}/evidence.json`);
if (error) throw error;

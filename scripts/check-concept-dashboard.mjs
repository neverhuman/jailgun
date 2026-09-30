// Full UI → Rust scheduling/storage → real browser adapter → synthetic provider.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { once } from 'node:events';
import { createInterface } from 'node:readline';
import { randomUUID, createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { chromium } from 'playwright-core';
import AxeBuilder from '@axe-core/playwright';
import { start } from '../apps/fake-chatgpt/src/server.mjs';

const viewer = process.argv[2] === 'viewer';
const mode = viewer ? 'viewer' : 'dashboard';
const provider = await start({ port: 0, interactive: true });
provider.concept.controls.generationMs = 3500;
provider.concept.controls.completionDelayMs = 300;
const chrome = process.env.JAILGUN_TEST_CHROME || chromium.executablePath();
try {
  for (const count of (viewer ? [5] : [5, 10])) {
    const output = resolve('target/concept-workflows', `${count}-${mode}-${randomUUID()}`);
    await mkdir(output, { recursive: true, mode: 0o700 });
    const child = spawn(resolve('target/debug/examples/account_browser_proof'), [provider.url, output, chrome, String(count), mode], { stdio: ['pipe', 'pipe', 'pipe'] });
    const exit = once(child, 'exit');
    const lines = createInterface({ input: child.stdout });
    let stderr = ''; child.stderr.on('data', data => { stderr = (stderr + data).slice(-12000); });
    let context;
    const result = { status: 'fail', candidate_count: count, checks: [], command: `node scripts/check-concept-dashboard.mjs${viewer ? ' viewer' : ''}`, generated_at: new Date().toISOString(), live_provider: false };
    try {
      const ready = await Promise.race([once(lines, 'line').then(([line]) => JSON.parse(line)), exit.then(() => { throw new Error('proof server exited before ready'); }), new Promise((_, reject) => { const t = setTimeout(() => reject(new Error('proof startup timed out')), 15000); t.unref(); })]);
      const url = ready.url;
      context = await chromium.launchPersistentContext(resolve(output, 'dashboard-browser'), { executablePath: chrome, headless: true, chromiumSandbox: true });
      const page = await context.newPage();
      page.setDefaultTimeout(15000);
      const errors = [];
      page.on('pageerror', error => errors.push(error.message));
      await page.goto(`${url}/#pair=${ready.pairing}&next=accounts`);
      await page.getByRole('heading', { name: 'Connect your first account' }).waitFor();
      assert.equal(new URL(page.url()).hash, '#accounts');
      assert.equal(await page.evaluate(() => localStorage.length), 0);
      await page.getByLabel('ChatGPT account email').fill('synthetic@example.invalid');
      await page.getByRole('button', { name: 'Connect ChatGPT' }).click();
      await page.locator('.status-waiting-for-user').waitFor();
      if (viewer) {
        await page.getByRole('button', { name: 'Open login view' }).click();
        await page.getByText('Connected. Complete ChatGPT login in the browser below.', { exact: true }).waitFor();
        const accountId = await page.locator('.accountCard').getAttribute('aria-label');
        const secondRejected = await page.evaluate(id => new Promise(resolve => {
          const socket = new WebSocket(`${location.origin.replace('http:', 'ws:')}/api/accounts/${encodeURIComponent(id)}/login-view`);
          const timeout = setTimeout(() => { socket.close(); resolve(false); }, 5000);
          socket.onopen = () => { clearTimeout(timeout); socket.close(); resolve(false); };
          socket.onerror = () => { clearTimeout(timeout); resolve(true); };
        }), accountId.slice('Account '.length));
        assert.equal(secondRejected, true);
        await page.getByRole('button', { name: 'Cancel login' }).click();
        await page.locator('.status-cancelled').waitFor();
        await page.locator('.loginScreen').waitFor({ state: 'hidden' });
        await page.getByRole('button', { name: 'Reconnect', exact: true }).click();
        await page.locator('.status-waiting-for-user').waitFor();
        // The account card retains the operator's choice to open the view.
        await page.getByText('Connected. Complete ChatGPT login in the browser below.', { exact: true }).waitFor();
        child.stdin.write('expire-login\n');
        await page.locator('.status-expired').waitFor();
        await page.locator('.loginScreen').waitFor({ state: 'hidden' });
        await page.getByRole('button', { name: 'Reconnect', exact: true }).click();
        await page.locator('.status-waiting-for-user').waitFor();
        result.checks.push('single-viewer', 'viewer-cancellation', 'viewer-expiry', 'reconnect-after-expiry');
        await page.getByText('Connected. Complete ChatGPT login in the browser below.', { exact: true }).waitFor();
        const canvas = page.locator('.loginScreen canvas');
        await page.waitForFunction(() => {
          const canvas = document.querySelector('.loginScreen canvas');
          if (!canvas?.width) return false;
          const pixels = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data;
          let bright = 0;
          for (let index = 0; index < pixels.length; index += 400) if (pixels[index] > 220 && pixels[index + 1] > 220 && pixels[index + 2] > 220) bright++;
          return bright > 100;
        });
        await canvas.screenshot({ path: resolve(output, 'login-browser.png') });
        result.login_screenshot_sha256 = createHash('sha256').update(await readFile(resolve(output, 'login-browser.png'))).digest('hex');
        result.viewer_ux = [];
        for (const [name, viewport] of Object.entries({ desktop: { width: 1440, height: 900 }, mobile: { width: 390, height: 844 } })) {
          await page.setViewportSize(viewport);
          await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
          await page.getByRole('button', { name: 'Use actual size' }).click();
          assert.equal(await page.getByRole('button', { name: 'Fit browser to view' }).getAttribute('aria-pressed'), 'true');
          assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
          await page.getByRole('button', { name: 'Fit browser to view' }).click();
          const axe = await new AxeBuilder({ page }).analyze();
          assert.deepEqual(axe.violations.map(v => ({ id: v.id, targets: v.nodes.map(n => n.target) })), []);
          assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
          await page.getByRole('button', { name: 'Focus login browser' }).click();
          await page.keyboard.press('Escape');
          assert.equal(await page.getByRole('button', { name: 'Focus login browser' }).evaluate(node => node === document.activeElement), true);
          const path = resolve(output, `login-${name}.png`);
          await page.screenshot({ path, fullPage: true });
          result.viewer_ux.push({ viewport, axe_violations: 0, screenshot_sha256: createHash('sha256').update(await readFile(path)).digest('hex') });
        }
        await page.setViewportSize({ width: 1440, height: 900 });
        await page.waitForFunction(() => {
          const canvas = document.querySelector('.loginScreen canvas');
          const box = canvas?.getBoundingClientRect();
          return canvas?.width === 1280 && canvas.height === 900 && box.width > 600 && Math.abs(box.width / box.height - 1280 / 900) < 0.01;
        });
        const box = await canvas.boundingBox();
        assert.ok(box);
        // Click noninteractive page text, then operate the provider's first
        // focusable control through RFB. No fixture-login command is used.
        await canvas.click({ position: { x: box.width * 400 / 1280, y: box.height * 160 / 900 } });
        await page.keyboard.press('Tab');
        await page.keyboard.press('Enter');
        await canvas.screenshot({ path: resolve(output, 'login-after-input.png') });
      } else child.stdin.write('login\n');
      await page.getByRole('button', { name: 'Confirm account and model' }).waitFor();
      await page.getByLabel('Model for new runs').selectOption('Fixture Two');
      await page.getByRole('button', { name: 'Confirm account and model' }).click();
      await page.locator('.status-ready').waitFor();
      if (viewer) { await page.getByRole('region', { name: 'Private ChatGPT login browser' }).waitFor({ state: 'hidden' }); result.checks.push('private-display-rfb-login', 'viewer-closed-after-confirmation'); }
      result.checks.push('cookie-pairing', 'manual-login-wait', 'observed-identity-and-explicit-model');
      await page.getByRole('link', { name: 'New concept', exact: true }).click();
      await page.getByLabel('What do you want to explore?').fill('A synthetic neighborhood lending library');
      const account = await page.getByLabel('ChatGPT account').locator('option').nth(1).getAttribute('value');
      await page.getByLabel('ChatGPT account').selectOption(account);
      await page.getByLabel('Perspectives', { exact: true }).selectOption(String(count));
      await page.getByRole('button', { name: 'Explore this concept' }).click();
      await page.getByRole('heading', { name: 'Run progress' }).waitFor();
      await page.getByRole('button', { name: 'Read final concept' }).waitFor({ timeout: 110000 });
      const runId = await page.getByLabel('Selected run').inputValue();
      const run = await page.evaluate(async id => (await fetch(`/api/runs/${id}`)).json(), runId);
      assert.equal(run.status, 'completed'); assert.equal(run.submissions, count + 4);
      assert.equal(run.tasks.filter(task => task.stage === 'explore' && task.status === 'completed').length, count);
      assert.deepEqual(run.configuration.model, { mode: 'specific', name: 'Fixture Two' });
      const attempts = await page.evaluate(async id => (await fetch(`/api/runs/${id}/attempts`)).json(), runId);
      assert.ok(attempts.every(attempt => attempt.state === 'completed' && attempt.observed_model === 'Fixture Two'));
      await page.getByRole('button', { name: 'Read final concept' }).click();
      await page.getByRole('article', { name: 'final.md' }).locator('.conceptMarkdown').waitFor();
      const final = run.artifacts.find(artifact => artifact.name === 'final.md');
      const expected = await page.evaluate(async ({ id, artifact }) => (await fetch(`/api/runs/${id}/artifacts/${artifact}`)).text(), { id: runId, artifact: final.id });
      assert.equal(createHash('sha256').update(expected).digest('hex'), final.sha256);
      assert.ok((await page.locator('.conceptMarkdown').textContent()).trim().length > 30);
      await page.getByLabel('Read an output').selectOption(run.artifacts.find(a => a.name === 'comparison.json').id);
      await page.getByRole('columnheader', { name: 'Weighted score / 100' }).waitFor();
      assert.equal(await page.locator('.ranking tbody tr').count(), count);
      await page.reload();
      await page.getByRole('heading', { name: 'Your final concept' }).waitFor();
      result.checks.push('five-stage-workflow', 'correct-candidate-count', 'observed-model', 'verified-final', 'comparison-ranking', 'session-reuse');
      assert.deepEqual(errors, []);
      await page.screenshot({ path: resolve(output, 'completed-dashboard.png'), fullPage: true });
      result.screenshot_sha256 = createHash('sha256').update(await readFile(resolve(output, 'completed-dashboard.png'))).digest('hex');
      result.status = 'pass';
    } catch (error) {
      result.error = error.message.replace(/[a-f0-9]{8}-[a-f0-9-]{27,}/g, '[redacted]');
      throw error;
    } finally {
      await context?.close(); lines.close(); child.stdin.end('stop\n');
      const stopTimer = setTimeout(() => child.kill('SIGTERM'), 10000); stopTimer.unref();
      const [code] = await exit; clearTimeout(stopTimer);
      result.exit_status = code;
      result.commit = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
      result.tree = execFileSync('git', ['rev-parse', 'HEAD^{tree}'], { encoding: 'utf8' }).trim();
      result.dirty = Boolean(execFileSync('git', ['status', '--porcelain'], { encoding: 'utf8' }).trim());
      result.node = process.version;
      result.chrome = execFileSync(chrome, ['--version'], { encoding: 'utf8' }).trim();
      const files = execFileSync('git', ['ls-files', '--cached', '--others', '--exclude-standard', '-z'], { encoding: 'utf8' }).split('\0').filter(file => /^(apps\/(dashboard|chrome-bridge|browser-adapter|fake-chatgpt)\/|crates\/jailgun-(workflow|orchestrator|server)\/|db\/migrations\/)/.test(file));
      result.input_sha256 = {};
      for (const file of [...files, 'Cargo.lock', 'package-lock.json', 'scripts/check-concept-dashboard.mjs', 'target/debug/examples/account_browser_proof']) result.input_sha256[file] = createHash('sha256').update(await readFile(file)).digest('hex');
      if (code !== 0) { result.status = 'fail'; result.server_error = stderr.replace(/[a-f0-9]{8}-[a-f0-9-]{27,}/g, '[redacted]'); }
      await writeFile(resolve(output, 'evidence.json'), JSON.stringify(result, null, 2) + '\n', { mode: 0o600 });
      if (code !== 0 && !result.error) throw new Error(`dashboard proof server failed: ${result.server_error}`);
    }
    console.log(`Synthetic ${count}-candidate ${mode} workflow passed: ${output}/evidence.json`);
  }
} finally { await provider.stop(); }

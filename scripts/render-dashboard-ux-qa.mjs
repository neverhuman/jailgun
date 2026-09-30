#!/usr/bin/env node
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { chromium } from 'playwright-core';
import AxeBuilder from '@axe-core/playwright';
import { preview } from 'vite';
import { conceptStates, conceptPage } from './lib/concept-ux-states.mjs';
import { observeLayoutStability, measureLayoutStability, verifyLayoutInstrumentation } from './lib/layout-stability.mjs';
import { fixtureRuns } from '../apps/dashboard/src/fixtures.ts';

const root = 'artifacts/ux-qa';
const reportPath = 'target/jankurai/ux-qa.json';
await mkdir(root, { recursive: true });
await mkdir('target/jankurai', { recursive: true });
const startedAt = new Date().toISOString();
const git = (...args) => execFileSync('git', args, { encoding: 'utf8' }).trim();
const source = { commit: git('rev-parse', 'HEAD'), tree: git('rev-parse', 'HEAD^{tree}'), dirty: Boolean(git('status', '--porcelain')) };
const inputs = [...new Set(git('ls-files', '--cached', '--others', '--exclude-standard', '-z').split('\0'))].filter(path => path.startsWith('apps/dashboard/') || path.startsWith('contracts/fixtures/') || path.startsWith('scripts/lib/') || ['package-lock.json', 'package.json', 'scripts/render-dashboard-ux-qa.mjs', 'agent/ux-qa.toml', 'ops/ci/ux-qa.sh'].includes(path));
for (const file of await readdir('apps/dashboard/dist', { recursive: true, withFileTypes: true })) {
  if (file.isFile()) inputs.push(`${file.parentPath}/${file.name}`);
}
source.input_sha256 = {};
for (const path of inputs.sort()) source.input_sha256[path] = createHash('sha256').update(await readFile(path)).digest('hex');
const reports = [];
const failures = [];
let server;
let context;
let layoutInstrumentation;
try {
  server = await preview({ root: resolve('apps/dashboard'), configFile: false, preview: { host: '127.0.0.1', port: 0 } });
  const address = server.httpServer.address();
  const base = `http://127.0.0.1:${address.port}`;
  context = await chromium.launchPersistentContext(resolve('target/ux-browser-profile'), {
    executablePath: process.env.JAILGUN_TEST_CHROME || undefined,
    headless: true,
    chromiumSandbox: true,
    timeout: 30_000,
  });
  layoutInstrumentation = await verifyLayoutInstrumentation(context);
  for (const [size, viewport] of Object.entries({ desktop: { width: 1440, height: 900 }, mobile: { width: 390, height: 844 } })) {
    for (const state of ['loading', 'empty', 'error', 'success', 'demo', 'pairing', ...conceptStates]) {
      const page = await context.newPage();
      page.setDefaultTimeout(10_000);
      await page.setViewportSize(viewport);
      await observeLayoutStability(page);
      const concept = conceptStates.includes(state);
      const errors = [];
      let paired = false;
      page.on('pageerror', error => errors.push(error.message));
      page.on('console', message => {
        if (message.type() === 'error' && !((['error', 'service-error'].includes(state) && /503/.test(message.text())) || (state === 'pairing' && /401/.test(message.text())))) errors.push(message.text());
      });
      page.on('requestfailed', request => errors.push(`${request.method()} ${new URL(request.url()).pathname}: ${request.failure()?.errorText}`));
      await page.routeWebSocket('**/ws/events', () => {});
      if (!concept) await page.route('**/api/**', async route => {
        await new Promise(done => setTimeout(done, 800));
        const path = new URL(route.request().url()).pathname;
        if (path === '/api/session/pair') { paired = true; return route.fulfill({ status: 204 }); }
        if (state === 'pairing' && !paired) return route.fulfill({ status: 401, json: { error: 'unauthorized' } });
        if (state === 'loading') return;
        if (state === 'error') return route.fulfill({ status: 503, json: { error: 'synthetic-unavailable' } });
        return route.fulfill({ json: path === '/api/runs' ? (state === 'empty' ? [] : fixtureRuns) : { run_id: fixtureRuns[0].run_id, receipts: [] } });
      });
      const result = { routeId: concept ? 'concept-dashboard' : 'dashboard', state, viewport, browserName: 'chromium', browserVersion: context.browser()?.version(), checkedAt: new Date().toISOString(), decision: 'fail', artifacts: [], checks: {} };
      try {
        if (concept) { await conceptPage(page, state, base); result.checks.keyboard = 'pass'; } else {
        await page.goto(`${base}/?advanced=1${state === 'demo' ? '&demo=1' : ''}`, { waitUntil: 'domcontentloaded' });
        if (state === 'loading') await page.getByRole('status').filter({ hasText: 'Loading runs' }).waitFor();
        else if (state === 'pairing') await page.getByRole('heading', { name: 'Pair this browser' }).waitFor();
        else if (state === 'empty') await page.getByRole('heading', { name: 'No runs yet' }).waitFor();
        else if (state === 'error') {
          await page.getByRole('alert').filter({ hasText: '503' }).waitFor();
          assert.equal(await page.getByText(/fixture-run/).count(), 0, 'API failure substituted demo data');
        } else {
          await page.getByRole('heading', { name: 'Jailgun', exact: true }).waitFor();
          await page.getByRole('link', { name: 'Agent summary' }).waitFor();
          if (state === 'demo') await page.getByText('Demo mode — synthetic data').waitFor();
          await page.keyboard.press('Tab');
          assert.equal(await page.evaluate(() => document.activeElement?.tagName), 'A', 'keyboard cannot reach primary link');
          result.checks.keyboard = 'pass';
        }
        }
        const geometry = await page.evaluate(() => ({ scrollWidth: document.documentElement.scrollWidth, clientWidth: document.documentElement.clientWidth }));
        assert.ok(geometry.scrollWidth <= geometry.clientWidth + 1, 'horizontal overflow');
        result.metrics = geometry;
        const axe = await new AxeBuilder({ page }).analyze();
        result.layoutStability = await measureLayoutStability(page);
        assert.ok(result.layoutStability.cls <= .1, `layout shift ${result.layoutStability.cls} exceeds the synthetic UX budget of 0.1`);
        const name = `dashboard-${size}-${state}`;
        const screenshot = `${root}/${name}.png`;
        const aria = `${root}/${name}.aria.yml`;
        const accessibility = `${root}/${name}.accessibility.json`;
        await page.screenshot({ path: screenshot, fullPage: true });
        await writeFile(aria, await page.locator('body').ariaSnapshot());
        await writeFile(accessibility, `${JSON.stringify(axe, null, 2)}\n`);
        for (const [kind, path] of [['screenshot', screenshot], ['aria-snapshot', aria], ['accessibility', accessibility]]) {
          result.artifacts.push({ kind, path, sha256: createHash('sha256').update(await readFile(path)).digest('hex') });
        }
        result.accessibility = { violations: axe.violations.length, incomplete: axe.incomplete.length, passes: axe.passes.length };
        result.consoleAndNetworkErrors = errors;
        assert.deepEqual(axe.violations.map(v => ({ id: v.id, impact: v.impact, nodes: v.nodes.map(n => n.target) })), [], 'accessibility violations');
        assert.deepEqual(errors, [], 'browser errors');
        if (state === 'pairing') {
          await page.getByLabel('Pairing code').fill('synthetic-one-use-code');
          await page.getByLabel('Pairing code').press('Enter');
          await page.getByRole('heading', { name: 'Jailgun', exact: true }).waitFor();
          await page.getByRole('link', { name: 'Agent summary' }).waitFor();
          result.checks.pairing = 'pass';
        }
        result.decision = 'pass';
      } catch (error) {
        result.error = error.message;
        failures.push(`${size}/${state}: ${error.message}`);
      } finally {
        reports.push(result);
        await page.close();
      }
    }
  }
} catch (error) {
  failures.push(error.message);
} finally {
  await context?.close();
  await new Promise(resolveClose => server?.httpServer.close(resolveClose) ?? resolveClose());
  for (const [path, hash] of Object.entries(source.input_sha256)) {
    try { if (createHash('sha256').update(await readFile(path)).digest('hex') !== hash) failures.push(`Input changed during UX checks: ${path}`); }
    catch { failures.push(`Input disappeared during UX checks: ${path}`); }
  }
  await writeFile(reportPath, `${JSON.stringify({ schema_version: 3, startedAt, finishedAt: new Date().toISOString(), source, command: 'node scripts/render-dashboard-ux-qa.mjs', node: process.version, layoutInstrumentation, status: failures.length ? 'fail' : 'pass', exit_status: failures.length ? 1 : 0, reports, failures }, null, 2)}\n`);
}
if (failures.length) {
  console.error(failures.join('\n'));
  process.exitCode = 1;
} else {
  console.log(`Passed ${reports.length} rendered dashboard checks; ${reportPath}`);
}

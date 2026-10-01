import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
const fixture = JSON.parse(await readFile(new URL('../../contracts/fixtures/workflow/states.json', import.meta.url), 'utf8'));

export const conceptStates = ['accounts-empty', 'accounts-verifying', 'new-concept', 'progress-partial', 'progress-complete', 'results-final', 'results-comparison', 'service-error', 'concept-demo'];
export async function conceptPage(page, state, base) {
  await page.route('**/api/**', async route => {
    await new Promise(done => setTimeout(done, 800));
    const path = new URL(route.request().url()).pathname;
    if (state === 'service-error') return route.fulfill({ status: 503, json: { code: 'service-unavailable', message: 'The local service is unavailable.', next_action: 'Reconnect to retry.' } });
    if (path === '/api/accounts') return route.fulfill({ json: state === 'accounts-empty' ? [] : fixture.accounts });
    if (path === '/api/accounts/sessions') return route.fulfill({ json: state === 'accounts-empty' ? [] : [fixture[state === 'accounts-verifying' ? 'account-verifying' : 'account-ready']] });
    const run = fixture[state === 'progress-partial' ? 'run-partial' : 'run-completed'];
    if (path === '/api/runs') return route.fulfill({ json: state === 'accounts-empty' ? [] : [run] });
    if (path.endsWith('/attempts')) return route.fulfill({ json: fixture[state === 'progress-partial' ? 'partial-attempts' : 'completed-attempts'] });
    if (path.endsWith('/events')) return route.fulfill({ json: fixture.events });
    if (path.includes('/artifacts/')) return route.fulfill({ body: fixture.artifacts[path.split('/').at(-1)], contentType: 'text/plain' });
    if (path === `/api/runs/${run.id}`) return route.fulfill({ json: run });
    throw new Error(`Unexpected UX request: ${path}`);
  });
  const hash = state.startsWith('progress-') ? 'runs' : state.startsWith('results-') || state === 'concept-demo' ? 'results' : state === 'new-concept' ? 'new' : 'accounts';
  await page.goto(`${base}/${state === 'concept-demo' ? '?demo=1' : ''}#${hash}`, { waitUntil: 'domcontentloaded' });
  if (state === 'service-error') {
    await page.getByRole('alert').filter({ hasText: 'local service is unavailable' }).waitFor();
    assert.equal(await page.getByText(/demo-account/).count(), 0);
  } else if (state === 'accounts-empty') await page.getByRole('heading', { name: 'Connect your first account' }).waitFor();
  else if (state === 'accounts-verifying') {
    await page.getByRole('button', { name: 'Confirm account and model' }).waitFor();
    await page.getByText('Current browser selection: Synthetic model', { exact: true }).waitFor();
    assert.equal(await page.locator('.primaryButton').count(), 1, 'account verification has competing primary actions');
  }
  else if (state === 'new-concept') {
    await page.getByLabel('What do you want to explore?').fill('A small neighborhood tool library');
    await page.getByLabel('ChatGPT account').selectOption('demo-account');
    await page.getByLabel('Perspectives', { exact: true }).selectOption('10');
  } else if (state === 'progress-partial') await page.getByRole('button', { name: 'Continue with 3 complete candidates' }).waitFor();
  else if (state === 'progress-complete') await page.getByRole('button', { name: 'Read final concept' }).waitFor();
  else {
    await page.getByRole('heading', { name: 'A small tool library, tested first' }).waitFor();
    if (state === 'results-comparison') {
      const artifact = fixture['run-completed'].artifacts.find(a => a.name === 'comparison.json');
      await page.getByLabel('Read an output').selectOption(artifact.id);
      await page.getByRole('columnheader', { name: 'Weighted score / 100' }).waitFor();
      const rank = page.getByRole('columnheader', { name: 'Rank', exact: true });
      assert.equal(await rank.evaluate(element => { const range = document.createRange();range.selectNodeContents(element);return range.getClientRects().length; }), 1, 'Rank wraps mid-word');
      const region = page.getByRole('region', { name: 'Candidate comparison', exact: true });
      if (await region.evaluate(element => element.scrollWidth > element.clientWidth)) {
        await page.getByText('Scroll across the table to read scores and rationales.', { exact: false }).waitFor();
        await region.focus();await page.keyboard.press('ArrowRight');
        await page.waitForFunction(() => document.querySelector('.ranking .tableScroll').scrollLeft > 0);
        await region.evaluate(element => new Promise(done => {
          let previous = element.scrollLeft, stable = 0;
          function settled() {
            stable = element.scrollLeft === previous ? stable + 1 : 0;
            previous = element.scrollLeft;
            if (stable >= 4) { element.scrollTo({ left: 0, behavior: 'instant' });done(); }
            else requestAnimationFrame(settled);
          }
          requestAnimationFrame(settled);
        }));
        assert.equal(await region.evaluate(element => element.scrollLeft), 0, 'comparison did not return to its first column');
      }
    }
  }
  const heading = await page.locator('#workspace h2').first().textContent();
  await page.keyboard.press('Control+Home');
  // Navigate from the browser's initial focus to the skip link, then enter the workspace.
  await page.evaluate(() => document.activeElement?.blur());
  await page.locator('.skipLink').focus();
  await page.keyboard.press('Enter');
  assert.equal(await page.evaluate(() => document.activeElement?.id), 'workspace');
  assert.equal(await page.locator('#workspace h2').first().textContent(), heading, 'skip link changed the active screen');
}

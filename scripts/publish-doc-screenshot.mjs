import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';

const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const report = JSON.parse(await readFile('target/jankurai/ux-qa.json', 'utf8'));
assert.equal(report.status, 'pass');
assert.equal(report.exit_status, 0);
const capture = report.reports.find(item => item.routeId === 'concept-dashboard' && item.state === 'results-final' && item.viewport.width === 1440);
assert.equal(capture?.decision, 'pass');
assert.equal(capture.accessibility.violations, 0);
assert.deepEqual(capture.consoleAndNetworkErrors, []);
const screenshot = capture.artifacts.find(item => item.kind === 'screenshot');
assert.equal(screenshot.path, 'artifacts/ux-qa/dashboard-desktop-results-final.png');
// Only this fixed synthetic UX state can be published. No browser profiles,
// arbitrary operator captures, private transcripts or raw diagnostics are inputs.
for (const [path, expected] of Object.entries(report.source.input_sha256)) {
  assert.equal(digest(await readFile(path)), expected, `Stale rendered input: ${path}`);
}
const bytes = await readFile(screenshot.path);
assert.equal(digest(bytes), screenshot.sha256, 'Screenshot differs from the executed UX evidence');
await writeFile('assets/dashboard-results.png', bytes);
await writeFile('assets/dashboard-results.json', `${JSON.stringify({
  generated_by: 'node scripts/publish-doc-screenshot.mjs',
  capture_command: report.command,
  content: 'Synthetic dashboard fixture, including an explicit three-candidate subset; not a live provider result',
  captured_at: capture.checkedAt,
  browser: { name: capture.browserName, version: capture.browserVersion },
  viewport: capture.viewport,
  source: {
    commit: report.source.commit, tree: report.source.tree, dirty: report.source.dirty,
    inputs: Object.entries(report.source.input_sha256).map(([path, sha256]) => ({ path, sha256 })),
  },
  screenshot: 'assets/dashboard-results.png',
  sha256: screenshot.sha256,
  check_status: capture.decision,
}, null, 2)}\n`);
console.log('Published verified synthetic dashboard screenshot and capture record');

import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { readFile, stat, writeFile } from 'node:fs/promises';

const execute = promisify(execFile);
const binary = '/home/test/installed application/bin/jailgun';
const runtime = '/home/test/private runtime';
const checks = [];
const request = (url, options = {}) => fetch(url, { ...options, signal: AbortSignal.timeout(10_000) });
const call = async args => JSON.parse((await execute(binary, [...args, '--runtime', runtime, '--json'], { timeout: 40_000 })).stdout);
for (const program of ['rustc', 'cargo', 'npm', 'node', 'git']) {
  await assert.rejects(execute('sh', ['-c', 'command -v "$1"', 'proof', program]));
}
await assert.rejects(stat('/home/ubuntu/jailgun'));
checks.push('no-source-or-development-toolchain');
let doctor;
try { await call(['doctor', '--headless']); }
catch (error) { doctor = JSON.parse(error.stdout); }
assert.equal(doctor.status, 'needs-attention');
assert.equal(doctor.checks.find(check => check.name === 'chrome').ok, false);
assert.ok(doctor.checks.find(check => check.name === 'chrome').next_action.includes('Install Google Chrome'));
assert.ok(doctor.checks.filter(check => check.name !== 'chrome').every(check => check.ok), JSON.stringify(doctor));
checks.push('actionable-missing-browser-and-complete-bundled-assets');
let started = false;
try {
  const setup = await call(['setup', '--headless', '--no-open']);
  started = true;
  const url = new URL(setup.dashboard_url);
  const origin = url.origin;
  const code = new URLSearchParams(url.hash.slice(1)).get('pair');
  const response = await request(`${origin}/api/session/pair`, { method: 'POST', headers: { origin, 'content-type': 'application/json' }, body: JSON.stringify({code}), signal: AbortSignal.timeout(10_000) });
  assert.equal(response.status, 204);
  const cookie = response.headers.getSetCookie()[0].split(';')[0];
  assert.equal((await request(`${origin}/api/accounts`)).status, 401);
  assert.deepEqual(await (await request(`${origin}/api/accounts`, {headers:{cookie}})).json(), []);
  assert.deepEqual(await call(['accounts', 'list']), []);
  const token = await readFile(`${runtime}/operator-token`);
  assert.equal((await stat(`${runtime}/operator-token`)).mode & 0o777, 0o600);
  const first = await call(['service', 'status']);
  await assert.rejects(execute(binary, ['uninstall', '--json'], {timeout:40_000}), error => JSON.parse(error.stderr).code === 'installation-busy');
  await call(['setup', '--headless', '--no-open']);
  assert.equal((await call(['service', 'status'])).instance_id, first.instance_id);
  assert.deepEqual(await readFile(`${runtime}/operator-token`), token);
  const html = await (await request(origin)).text();
  const path = html.match(/src="(\/assets\/[^\"]+\.js)"/)?.[1];
  assert.ok(path);
  assert.equal((await request(`${origin}${path}`)).status, 200);
  checks.push('setup-pairing-private-api-dashboard-and-daemon-reuse');
  assert.equal((await call(['service', 'stop'])).state, 'stopped');
  assert.equal((await call(['service', 'stop'])).state, 'stopped');
  started = false;
  const result = JSON.parse((await execute(binary, ['data', 'backup', '--runtime', runtime, '--out', '/home/test/backup', '--json'], {timeout:40_000})).stdout);
  assert.equal(result.status, 'completed');
  const restored = JSON.parse((await execute(binary, ['data', 'restore', '--backup', '/home/test/backup', '--out', '/home/test/restored', '--json'], {timeout:40_000})).stdout);
  assert.equal(restored.status, 'completed');
  checks.push('authenticated-stop-and-offline-backup-restore');
  const bundle = JSON.parse(await readFile('/home/test/installed application/lib/jailgun/current/bundle.json', 'utf8'));
  const data = await readFile(`${runtime}/workflows.sqlite3`);
  const removed = JSON.parse((await execute(binary, ['uninstall', '--json'], {timeout:40_000})).stdout);
  assert.equal(removed.status, 'completed');assert.equal(removed.runtime_data, 'unchanged');
  await assert.rejects(stat(binary), {code:'ENOENT'});
  await assert.rejects(stat('/home/test/installed application/lib/jailgun'), {code:'ENOENT'});
  assert.deepEqual(await readFile(`${runtime}/workflows.sqlite3`), data);
  assert.deepEqual(await readFile(`${runtime}/operator-token`), token);
  checks.push('active-use-refusal-and-uninstall-retains-private-data');
  await writeFile('/evidence/result.json', `${JSON.stringify({generated_at:new Date().toISOString(),status:'pass',checks,source:bundle.source,version:bundle.version,platform:bundle.platform,node:process.version,os:await readFile('/etc/os-release','utf8'),installation_source_present:false,live_provider_verified:false,caveats:['No browser was installed; full synthetic-browser and live-provider acceptance are separate checks.','No native service manager was started in this container.']}, null, 2)}\n`,{mode:0o600});
} finally {
  if(started) await call(['service','stop']);
}

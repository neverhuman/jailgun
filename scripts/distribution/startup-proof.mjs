import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { mkdir, readFile, realpath, stat, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { promisify } from 'node:util';

const execute = promisify(execFile);
const command = (program, args, options = {}) => execute(program, args, { timeout: 75_000, maxBuffer: 1024 * 1024, ...options });

// A real installed daemon, managed by an isolated native user job. No account is
// connected and no live provider request is submitted. Missing managers fail.
export async function checkStartup({ binary, proof, run, env, working }) {
  const mac = process.platform === 'darwin';
  assert.ok(mac || process.platform === 'linux', 'Unsupported native service manager');
  const name = `jailgun-proof-${randomUUID()}`;
  const filename = `${name}.${mac ? 'plist' : 'service'}`;
  const unit = join(proof, filename);
  const runtime = join(proof, 'startup runtime $ % " & \\');
  const listener = createServer();await new Promise(done => listener.listen(0, '127.0.0.1', done));
  const address = `127.0.0.1:${listener.address().port}`;
  await new Promise(done => listener.close(done));
  await mkdir(runtime, { mode: 0o700 });
  await writeFile(join(runtime, 'daemon.json'), JSON.stringify({url:`http://${address}`,pid:0,version:'synthetic-startup-proof'}), {mode:0o600});
  const generated = JSON.parse(await run(['service', 'unit', '--out', unit, '--runtime', runtime, '--headless', '--addr', address, '--json']));
  assert.equal(generated.activated, false);
  assert.equal(generated.status, 'written');
  assert.equal((await stat(unit)).mode & 0o777, 0o600);
  const text = await readFile(unit, 'utf8');
  const label = mac ? `gui/${process.getuid()}/${name}` : filename;
  const native = (args) => command(mac ? '/bin/launchctl' : 'systemctl', mac ? args : ['--user', ...args]);
  // Actual daemon startup proves native argument parsing, including special
  // characters. Separately require the stable installation link across upgrades.
  if (mac) {
    await command('/usr/bin/plutil', ['-lint', unit]);
    const parsed = JSON.parse((await command('/usr/bin/plutil', ['-convert', 'json', '-o', '-', unit])).stdout);
    assert.equal(parsed.Label, name);
    assert.equal(parsed.ProgramArguments[0], binary);
    assert.equal(parsed.ProgramArguments[parsed.ProgramArguments.indexOf('--runtime') + 1], runtime);
    await assert.rejects(native(['print', label]), 'Refuse to replace an existing job');
  } else {
    const quotedBinary = binary.replaceAll('\\', '\\\\').replaceAll('"', '\\"').replaceAll('%', '%%').replaceAll('$', () => '$$');
    assert.ok(text.includes(`ExecStart=/usr/bin/env -- "${quotedBinary}"`), 'Use the stable installed link');
    await command('systemd-analyze', ['--user', 'verify', unit]);
    assert.equal((await native(['show', '--property=LoadState', '--value', filename])).stdout.trim(), 'not-found');
  }
  const cli = async (...args) => JSON.parse((await command(binary, [...args, '--runtime', runtime, '--json'], { cwd: working, env })).stdout);
  const observe = async (predicate, milliseconds = 35_000) => {
    const deadline = Date.now() + milliseconds;
    while (Date.now() < deadline) {
      try {
        const status = await cli('service', 'status');
        if (predicate(status)) return status;
      } catch (error) {
        if (!error.stderr?.includes('daemon-unavailable')) throw error;
      }
      await delay(200);
    }
    throw new Error('Native service failed to reach the expected authenticated state');
  };
  let attached = false;
  try {
    if (mac) {
      await native(['bootstrap', `gui/${process.getuid()}`, unit]);
      attached = true;
    } else {
      await native(['link', '--runtime', '--', unit]);
      attached = true;
      assert.equal(await realpath((await native(['show', '--property=FragmentPath', '--value', filename])).stdout.trim()), unit);
      await native(['start', '--', filename]);
    }
    const first = await observe(status => status.state === 'running');
    assert.equal(first.runtime, runtime);
    assert.match(first.instance_id, /^[a-f0-9-]{36}$/);
    assert.equal((await stat(join(runtime, 'daemon.log'))).mode & 0o777, 0o600);
    const token = (await readFile(join(runtime, 'operator-token'), 'utf8')).trim();
    assert.ok(!text.includes(token), 'Credential must not be in the service definition');
    // Signal only the exact newly registered native job, never a saved PID.
    await native(mac ? ['kill', 'SIGKILL', label] : ['kill', '--kill-whom=main', '--signal=KILL', '--', filename]);
    const restarted = await observe(status => status.state === 'running' && status.instance_id !== first.instance_id);
    assert.equal(restarted.runtime, runtime);
    assert.equal((await readFile(join(runtime, 'operator-token'), 'utf8')).trim(), token);
    assert.equal((await cli('service', 'stop')).state, 'stopped');
    // Observe longer than the ten-second failure restart delay to catch an
    // unintended restart after an explicit successful stop.
    const deadline = Date.now() + 12_000;
    while (Date.now() < deadline) {
      assert.equal((await cli('service', 'status')).state, 'stopped');
      await delay(500);
    }
  } finally {
    if (attached) {
      if (mac) {
        await native(['bootout', label]);
        await assert.rejects(native(['print', label]), 'Proof job must be unloaded');
      } else {
        assert.equal(await realpath((await native(['show', '--property=FragmentPath', '--value', filename])).stdout.trim()), unit, 'Refuse cleanup of a foreign definition');
        await native(['stop', '--', filename]);
        await native(['disable', '--runtime', '--', filename]);
        await native(['daemon-reload']);
        assert.equal((await native(['show', '--property=LoadState', '--value', filename])).stdout.trim(), 'not-found');
      }
    }
  }
  assert.equal((await cli('service', 'status')).state, 'stopped');
  return [`${mac ? 'launchd' : 'systemd-user'}-native-validation-start-failure-restart-operator-stop-and-unload`];
}

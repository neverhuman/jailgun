import assert from 'node:assert/strict';
import { mkdir, readFile, writeFile, stat } from 'node:fs/promises';
import { createServer } from 'node:net';
import { join } from 'node:path';
import { fileHash } from './files.mjs';

export async function checkRemoval({ binary, installation, installed, runtime, proof, run }) {
  const second = join(proof, 'second private runtime');
  await mkdir(second, { mode: 0o700 });
  const listener = createServer();await new Promise(done => listener.listen(0, '127.0.0.1', done));
  const url = `http://127.0.0.1:${listener.address().port}`;
  await new Promise(done => listener.close(done));
  const version = JSON.parse(await readFile(join(installed, 'bundle.json'), 'utf8')).version;
  await writeFile(join(second, 'daemon.json'), JSON.stringify({ url, pid: 0, version }), { mode: 0o600 });
  let started = false;
  try {
    await run(['setup', '--runtime', second, '--headless', '--no-open', '--json']);started = true;
    await assert.rejects(run(['uninstall', '--json']), error => JSON.parse(error.stderr).code === 'installation-busy');
    assert.equal(JSON.parse(await run(['service', 'status', '--runtime', second, '--json'])).state, 'running');
  } finally {
    if (started) assert.equal(JSON.parse(await run(['service', 'stop', '--runtime', second, '--json'])).state, 'stopped');
  }
  const original = await readFile(join(installed, 'LICENSE'));
  await writeFile(join(installed, 'LICENSE'), 'modified application file');
  await assert.rejects(run(['uninstall', '--json']), error => JSON.parse(error.stderr).code === 'installation-invalid');
  assert.ok((await stat(binary)).isFile());
  await writeFile(join(installed, 'LICENSE'), original);
  const retained = ['operator-token', 'workflows.sqlite3'];
  const hashes = await Promise.all(retained.map(name => fileHash(join(runtime, name))));
  await writeFile(join(installation, 'bin/unrelated-tool'), 'retained unrelated application');
  const removed = JSON.parse(await run(['uninstall', '--json']));
  assert.equal(removed.status, 'completed');assert.equal(removed.runtime_data, 'unchanged');
  await assert.rejects(stat(join(installation, 'lib/jailgun')), { code: 'ENOENT' });
  await assert.rejects(stat(binary), { code: 'ENOENT' });
  assert.equal(await readFile(join(installation, 'bin/unrelated-tool'), 'utf8'), 'retained unrelated application');
  assert.deepEqual(await Promise.all(retained.map(name => fileHash(join(runtime, name)))), hashes);
  return ['another-runtime-blocks-uninstall', 'modified-installation-is-retained', 'uninstall-preserves-accounts-results-and-unrelated-tools'];
}

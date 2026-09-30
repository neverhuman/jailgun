import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { mkdir, mkdtemp, writeFile, readFile, symlink } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { sourceSnapshot } from './source-sbom.mjs';

test('source SBOM snapshot excludes ignored profiles, local config and test installations', async () => {
  const parent = resolve('target/security-snapshot-proofs');await mkdir(parent, { recursive: true });
  const root = await mkdtemp(join(parent, 'fixture-'));
  const git = args => promisify(execFile)('git', args, { cwd: root, timeout: 10_000 });
  await git(['init', '-q']);await writeFile(join(root, '.gitignore'), 'target/\nprofiles/\n*.local.toml\n');
  await writeFile(join(root, 'package.json'), '{"name":"synthetic-source","version":"1.0.0"}\n');
  await git(['add', '.']);await git(['-c', 'user.name=Synthetic Fixture', '-c', 'user.email=fixture@example.invalid', '-c', 'commit.gpgSign=false', 'commit', '-qm', 'source fixture']);
  for (const path of ['target/installed/package.json', 'profiles/private/package.json', 'operator.local.toml']) {
    const full = join(root, path);await mkdir(full.slice(0, full.lastIndexOf('/')), { recursive: true });await writeFile(full, 'PRIVATE-SYNTHETIC-CANARY');
  }
  await writeFile(join(root, 'new-source.mjs'), 'export const fixture = true;\n');
  const destination = join(root, 'target/snapshot');
  const snapshot = await sourceSnapshot(root, destination);
  assert.deepEqual(Object.keys(snapshot.input_sha256), ['.gitignore', 'new-source.mjs', 'package.json']);
  assert.equal(snapshot.dirty, true);
  assert.equal(await readFile(join(destination, 'package.json'), 'utf8'), '{"name":"synthetic-source","version":"1.0.0"}\n');
  await symlink('profiles/private/package.json', join(root, 'linked-package.json'));
  await assert.rejects(sourceSnapshot(root, join(root, 'target/unsafe')), /links are not followed/);
});

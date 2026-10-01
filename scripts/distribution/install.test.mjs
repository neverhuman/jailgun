import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, readlink, readdir, writeFile, symlink, lstat, unlink } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { installBundle } from './install.mjs';
import { files, json } from './files.mjs';
import { fixture } from './fixture.mjs';

async function prefix(bundle) { return mkdtemp(join(dirname(bundle), 'prefix with spaces-')); }
test('installs and upgrades atomically, retaining old assets and runtime data', async () => {
  const first = await fixture();const destination = await prefix(first);
  const data = join(destination, '.jailgun');await mkdir(data);await writeFile(join(data, 'profile-marker'), 'private synthetic data');
  const installed = await installBundle(first, destination, 'a'.repeat(64));
  assert.equal(await readlink(join(destination, 'bin/jailgun')), '../lib/jailgun/current/bin/jailgun');
  const base = join(destination, 'lib/jailgun');
  const useLock = await lstat(join(base, '.usage.lock'));
  assert.equal(useLock.mode & 0o777, 0o600);
  assert.equal(await readlink(join(base, 'current')), `releases/${installed.release}`);
  const second = await fixture();const metadata = JSON.parse(await readFile(join(second, 'bundle.json'), 'utf8'));
  metadata.version = '0.0.1-test';await json(join(second, 'bundle.json'), metadata);
  await json(join(second, 'manifest.json'), { schema_version: 1, files: (await files(second)).filter(file => file.path !== 'manifest.json') });
  const upgraded = await installBundle(second, destination, 'b'.repeat(64));
  assert.equal((await lstat(join(base, '.usage.lock'))).ino, useLock.ino, 'Upgrade must retain the shared lock inode');
  const registry = JSON.parse(await readFile(join(base, 'installation.json'), 'utf8'));
  assert.ok(Object.values(registry.releases).every(release => release.usage_lock_version === 1));
  assert.equal(await readlink(join(base, 'current')), `releases/${upgraded.release}`);
  assert.equal((await readdir(join(base, 'releases'))).length, 2);
  assert.equal(await readFile(join(data, 'profile-marker'), 'utf8'), 'private synthetic data');
  assert.equal((await installBundle(second, destination, 'b'.repeat(64))).release, upgraded.release);
  await writeFile(join(base, 'releases', upgraded.release, 'LICENSE'), 'modified');
  await assert.rejects(installBundle(second, destination, 'b'.repeat(64)), /do not match/);
});

test('refuses linked process-use locks and interrupted removal journals during upgrades', async () => {
  for (const kind of ['linked-lock', 'journal']) {
    const source = await fixture();const destination = await prefix(source);
    const installed = await installBundle(source, destination, 'a'.repeat(64));
    const base = join(destination, 'lib/jailgun');
    if (kind === 'linked-lock') {
      await unlink(join(base, '.usage.lock'));
      await writeFile(join(destination, 'foreign'), 'retained');
      await symlink(join(destination, 'foreign'), join(base, '.usage.lock'));
    } else await writeFile(join(base, '.uninstall.json'), '{}');
    await assert.rejects(installBundle(source, destination, 'b'.repeat(64)), /lock must be a regular file|interrupted uninstall/);
    assert.equal(await readlink(join(base, 'current')), `releases/${installed.release}`);
    assert.equal((await readdir(join(base, 'releases'))).length, 1);
    if (kind === 'linked-lock') assert.equal(await readFile(join(destination, 'foreign'), 'utf8'), 'retained');
  }
});
test('refuses foreign executables, directories, links and an active installer', async () => {
  for (const kind of ['executable', 'directory', 'link', 'locked']) {
    const source = await fixture();const destination = await prefix(source);
    await mkdir(join(destination, 'bin'));await mkdir(join(destination, 'lib/jailgun'), { recursive: true });
    if (kind === 'executable') await writeFile(join(destination, 'bin/jailgun'), 'unrelated tool');
    if (kind === 'directory') await writeFile(join(destination, 'lib/jailgun/foreign'), 'unrelated files');
    if (kind === 'link') await symlink('/unrelated/tool', join(destination, 'bin/jailgun'));
    if (kind === 'locked') await mkdir(join(destination, 'lib/jailgun/.install-lock'));
    await assert.rejects(installBundle(source, destination, 'a'.repeat(64)), /Refusing|locked/);
    if (kind === 'executable') assert.equal(await readFile(join(destination, 'bin/jailgun'), 'utf8'), 'unrelated tool');
    if (kind === 'locked') assert.ok((await readdir(join(destination, 'lib/jailgun'))).includes('.install-lock'));
  }
});
test('rejects corrupt upgrades before changing the active version and refuses recursive installation', async () => {
  const source = await fixture();const destination = await prefix(source);
  const installed = await installBundle(source, destination, 'a'.repeat(64));
  await writeFile(join(source, 'LICENSE'), 'corrupt');
  await assert.rejects(installBundle(source, destination, 'b'.repeat(64)), /do not match/);
  assert.equal(await readlink(join(destination, 'lib/jailgun/current')), `releases/${installed.release}`);
  const recursive = await fixture();
  await assert.rejects(installBundle(recursive, recursive, 'c'.repeat(64)), /outside the managed installation/);
});

test('rejects backslash installation paths that the Node ESM loader cannot execute', async () => {
  const source = await fixture();
  const parent = await prefix(source);
  const destination = join(parent, 'prefix with \\');
  await assert.rejects(installBundle(source, destination, 'a'.repeat(64)), /Installation paths cannot contain/);
  await mkdir(destination, { recursive: true });
  const alias = join(parent, 'normal alias');
  await symlink(destination, alias);
  await assert.rejects(installBundle(source, join(alias, 'child'), 'a'.repeat(64)), /Installation paths cannot contain/);
  assert.ok(!(await readdir(destination)).includes('lib'));
});

import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { lstat, mkdir, readFile, readdir, readlink, realpath, rename, rm, symlink, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { copy, fileHash } from './files.mjs';
import { verifyBundle } from './verify.mjs';

async function entry(path) {
  try { return await lstat(path); } catch (error) { if (error.code === 'ENOENT') return undefined;throw error; }
}
async function directory(path) {
  await mkdir(path, { recursive: true, mode: 0o755 });
  assert.ok((await lstat(path)).isDirectory(), `Installation directory is a link or special file: ${path}`);
}
async function atomicJson(path, value) {
  const pending = `${path}.${randomUUID()}.pending`;
  await writeFile(pending, `${JSON.stringify(value, null, 2)}\n`, { flag: 'wx', mode: 0o644 });
  await rename(pending, path);
}
const linkTarget = name => `../lib/jailgun/current/bin/${name}`;
function validatePrefix(path) {
  assert.ok(path && !/[\x00-\x1f\x7f\\]/.test(path), 'Installation paths cannot contain control characters or backslashes; Node cannot load runtime modules from a backslash path');
}

export async function installBundle(bundle, requestedPrefix, archiveHash) {
  assert.match(archiveHash, /^[a-f0-9]{64}$/, 'Expected verified archive SHA-256');
  validatePrefix(requestedPrefix);
  validatePrefix(resolve(requestedPrefix));
  bundle = await realpath(bundle);
  const report = await verifyBundle(bundle);
  await directory(resolve(requestedPrefix));
  const prefix = await realpath(requestedPrefix);
  validatePrefix(prefix);
  assert.ok(prefix !== bundle && !prefix.startsWith(`${bundle}/`) && !bundle.startsWith(`${prefix}/lib/jailgun/`), 'Stage the bundle outside the managed installation');
  await directory(join(prefix, 'bin'));
  await directory(join(prefix, 'lib'));
  const base = join(prefix, 'lib/jailgun');await directory(base);
  const lock = join(base, '.install-lock');
  try { await mkdir(lock, { mode: 0o700 }); }
  catch (error) { throw new Error(`Installation is locked at ${lock}. Check for an active installer before removing a stale lock.`, { cause: error }); }
  try {
    await writeFile(join(lock, 'owner.json'), JSON.stringify({ pid: process.pid, started_at: new Date().toISOString() }), { mode: 0o600 });
    assert.ok(!await entry(join(base, '.uninstall.json')), 'An interrupted uninstall must finish before installation');
    const marker = join(base, 'installation.json');
    let state;
    if (await entry(marker)) {
      assert.ok((await lstat(marker)).isFile(), 'Installation marker must be a regular file');
      state = JSON.parse(await readFile(marker, 'utf8'));
      assert.equal(state.schema_version, 1);assert.equal(state.application, 'jailgun');assert.equal(state.prefix, prefix);
      assert.ok(state.releases && typeof state.releases === 'object' && !Array.isArray(state.releases));
    } else {
      assert.deepEqual(await readdir(base), ['.install-lock'], 'Refusing to adopt an unregistered application directory');
      state = { schema_version: 1, application: 'jailgun', prefix, releases: {} };
      await atomicJson(marker, state);
    }
    const usageLock = join(base, '.usage.lock');
    if (await entry(usageLock)) assert.ok((await lstat(usageLock)).isFile(), 'Application-use lock must be a regular file');
    else await writeFile(usageLock, '', { flag: 'wx', mode: 0o600 });
    for (const name of ['jailgun', 'jailhard']) {
      const existing = await entry(join(prefix, 'bin', name));
      assert.ok(!existing || existing.isSymbolicLink() && await readlink(join(prefix, 'bin', name)) === linkTarget(name), `Refusing to replace an unrelated ${name} executable`);
    }
    const current = join(base, 'current');
    const previous = await entry(current);
    if (previous) {
      assert.ok(previous.isSymbolicLink(), 'Active installation must be a managed link');
      const target = await readlink(current);
      assert.ok(/^releases\/[A-Za-z0-9.-]+$/.test(target) && Object.hasOwn(state.releases, target.slice(9)), 'Active link points outside the registered releases');
    }
    await directory(join(base, 'releases'));
    const release = `${report.version}-${report.platform}-${archiveHash}`;
    const destination = join(base, 'releases', release);
    const manifestHash = await fileHash(join(bundle, 'manifest.json'));
    if (await entry(destination)) {
      assert.ok((await lstat(destination)).isDirectory(), 'Release directory must not be a link');
      assert.equal(state.releases[release]?.manifest_sha256, manifestHash, 'Release identity conflicts with this archive');
      await verifyBundle(destination);
      assert.equal(await fileHash(join(destination, 'manifest.json')), manifestHash, 'Installed manifest differs');
    } else {
      const staging = join(base, 'releases', `.pending-${randomUUID()}`);
      await mkdir(staging, { mode: 0o700 });
      // Retain an interrupted copy for diagnosis; it is never activated.
      await copy(bundle, staging);
      await verifyBundle(staging);
      assert.equal(await fileHash(join(staging, 'manifest.json')), manifestHash, 'Staged manifest changed');
      state.releases[release] = { archive_sha256: archiveHash, manifest_sha256: manifestHash, usage_lock_version: report.usage_lock_version, installed_at: new Date().toISOString(), source: report.source };
      await atomicJson(marker, state);
      await rename(staging, destination);
    }
    for (const name of ['jailgun', 'jailhard']) {
      if (!await entry(join(prefix, 'bin', name))) await symlink(linkTarget(name), join(prefix, 'bin', name));
    }
    const pendingLink = join(base, `.current-${randomUUID()}`);
    await symlink(`releases/${release}`, pendingLink);
    await rename(pendingLink, current);
    return { status: 'installed', version: report.version, prefix, release, previous_release: previous ? 'retained' : null, runtime_data: 'unchanged' };
  } finally { await rm(lock, { recursive: true }); }
}

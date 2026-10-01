import assert from 'node:assert/strict';
import { readFile, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { files } from './files.mjs';

async function document(path) {
  assert.ok((await stat(path)).size <= 8 * 1024 * 1024, 'Bundle metadata exceeds its size limit');
  return JSON.parse(await readFile(path, 'utf8'));
}
export async function verifyBundle(root) {
  // Walk before reading metadata: links and special files are never followed.
  const actual = await files(root);
  const manifest = await document(join(root, 'manifest.json'));
  assert.equal(manifest.schema_version, 1);
  assert.ok(Array.isArray(manifest.files) && manifest.files.length > 0 && manifest.files.length < 100_000, 'Invalid bundle manifest');
  const seen = new Set();
  for (const entry of manifest.files) {
    assert.equal(typeof entry.path, 'string');
    assert.ok(!entry.path.startsWith('/') && !/[\x00-\x1f\\]/.test(entry.path) && entry.path.split('/').every(part => part && part !== '.' && part !== '..'), 'Unsafe manifest path');
    assert.ok(!seen.has(entry.path) && entry.path !== 'manifest.json', 'Duplicate or recursive manifest entry');seen.add(entry.path);
  }
  assert.deepEqual(actual.filter(file => file.path !== 'manifest.json'), manifest.files, 'Bundle contents do not match the manifest');
  for (const required of ['bin/jailgun', 'bin/jailhard', 'lib/jailgun/node/bin/node', 'lib/jailgun/node/LICENSE', 'lib/jailgun/apps/chrome-bridge/bin/chrome-bridge.mjs', 'lib/jailgun/apps/chrome-bridge/bin/concept-bridge.mjs', 'lib/jailgun/apps/dashboard/dist/index.html', 'lib/jailgun/config/jailgun.example.toml', 'lib/jailgun/install-bundle.mjs', 'share/jailgun/notices/index.json', 'share/jailgun/sbom.cdx.json', 'LICENSE', 'bundle.json', 'install.sh']) {
    assert.ok(seen.has(required), `Missing runtime asset: ${required}`);
  }
  for (const executable of ['bin/jailgun', 'bin/jailhard', 'lib/jailgun/node/bin/node']) assert.ok(actual.find(file => file.path === executable).executable, `Not executable: ${executable}`);
  const metadata = await document(join(root, 'bundle.json'));
  assert.equal(metadata.schema_version, 1);
  assert.match(metadata.version, /^\d+\.\d+\.\d+(?:-[A-Za-z0-9.-]+)?$/);
  assert.equal(metadata.platform, `${process.platform}-${process.arch}`, 'Bundle belongs to another platform');
  assert.equal(`v${metadata.node.version}`, process.version, 'Use the bundle’s pinned Node runtime');
  assert.match(metadata.source.commit, /^[a-f0-9]{40}$/);
  assert.match(metadata.source.tree, /^[a-f0-9]{40}$/);
  assert.equal(typeof metadata.source.dirty, 'boolean');
  const usageLockVersion = metadata.usage_lock_version ?? 0;
  assert.ok([0, 1].includes(usageLockVersion), 'Unsupported application-use lock protocol');
  return { status: 'verified', version: metadata.version, platform: metadata.platform, source: metadata.source, usage_lock_version: usageLockVersion, file_count: actual.length };
}

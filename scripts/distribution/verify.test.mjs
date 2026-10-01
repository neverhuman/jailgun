import { test } from 'node:test';
import assert from 'node:assert/strict';
import { writeFile, chmod, symlink, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { files, json } from './files.mjs';
import { verifyBundle } from './verify.mjs';

import { fixture } from './fixture.mjs';

test('verifies every regular file and executable bit', async () => {
  const root = await fixture();assert.equal((await verifyBundle(root)).status, 'verified');
  await chmod(join(root, 'bin/jailgun'), 0o644);
  await assert.rejects(verifyBundle(root), /do not match/);
});
test('rejects corruption and undeclared additions', async () => {
  for (const file of ['LICENSE', 'unregistered']) {
    const root = await fixture();await writeFile(join(root, file), 'altered');
    await assert.rejects(verifyBundle(root), /do not match/);
  }
});
test('rejects links without reading their target', async () => {
  const root = await fixture();await symlink('/absent-outside-bundle', join(root, 'outside'));
  await assert.rejects(verifyBundle(root), /link or special file/);
});
test('rejects escaping, duplicate and self-referencing manifest paths', async () => {
  for (const path of ['../outside', '/outside', 'manifest.json', 'LICENSE']) {
    const root = await fixture();const manifest = JSON.parse(await readFile(join(root, 'manifest.json'), 'utf8'));
    manifest.files.push({ ...manifest.files[0], path });await json(join(root, 'manifest.json'), manifest);
    await assert.rejects(verifyBundle(root), /Unsafe|Duplicate or recursive/);
  }
});
test('rejects a correctly hashed bundle for the wrong platform', async () => {
  const root = await fixture();const metadata = JSON.parse(await readFile(join(root, 'bundle.json'), 'utf8'));
  metadata.platform = 'unverified-platform';await json(join(root, 'bundle.json'), metadata);
  await json(join(root, 'manifest.json'), { schema_version: 1, files: (await files(root)).filter(file => file.path !== 'manifest.json') });
  await assert.rejects(verifyBundle(root), /another platform/);
});

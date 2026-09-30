import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { mkdtemp, mkdir, symlink, writeFile, lstat } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileHash } from './files.mjs';

const execute = promisify(execFile);
test('shell installer rejects checksum-valid linked and duplicate archive entries before installation', async () => {
  const parent = resolve('target/distribution-verifier-proofs');await mkdir(parent, { recursive: true });
  for (const kind of ['symlink', 'duplicate']) {
    const root = await mkdtemp(join(parent, 'archive-'));const input = join(root, 'input');await mkdir(input);
    await writeFile(join(input, 'file'), 'untrusted archive fixture');
    const entries = kind === 'duplicate' ? ['./file', './file'] : ['./file', './outside'];
    if (kind === 'symlink') await symlink('../outside-installation', join(input, 'outside'));
    const archive = join(root, `jailgun-0.0.0-test-${process.platform}-${process.arch}.tar.gz`);
    await execute('tar', ['-czf', archive, '-C', input, ...entries], { timeout: 10_000 });
    const sums = join(root, 'SHA256SUMS');await writeFile(sums, `${await fileHash(archive)}  ${archive.split('/').at(-1)}\n`);
    const prefix = join(root, 'must not be created');
    await assert.rejects(execute('bash', [resolve('scripts/install.sh'), '--archive', archive, '--checksums', sums, '--prefix', prefix], { env: { ...process.env, HOME: root, XDG_CACHE_HOME: join(root, 'cache') }, timeout: 10_000 }), kind === 'duplicate' ? /duplicate archive paths/ : /links and special archive files/);
    await assert.rejects(lstat(prefix), { code: 'ENOENT' });
  }
});

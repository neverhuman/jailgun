import { test } from 'node:test';
import assert from 'node:assert/strict';
import { build } from 'esbuild';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { mkdtemp } from 'node:fs/promises';
import { join, dirname } from 'node:path';
import { fixture } from './fixture.mjs';

test('compiled installer does not execute its imported verifier command entrypoint', async () => {
  const bundle = await fixture();const parent = await mkdtemp(join(dirname(bundle), 'entrypoints-'));
  for (const name of ['install', 'verify']) {
    const outfile = join(parent, `${name}.mjs`);
    await build({ entryPoints: [`scripts/distribution/${name}-cli.mjs`], outfile, bundle: true, platform: 'node', target: 'node24', format: 'esm' });
    const args = name === 'verify' ? [bundle] : [bundle, join(parent, 'installed with spaces'), 'a'.repeat(64)];
    const result = JSON.parse((await promisify(execFile)(process.execPath, [outfile, ...args], { timeout: 10_000 })).stdout);
    assert.equal(result.status, name === 'verify' ? 'verified' : 'installed');
  }
});

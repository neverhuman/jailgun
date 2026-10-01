import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { access, mkdir, readFile, rename } from 'node:fs/promises';
import { join } from 'node:path';
import { fileHash } from './files.mjs';

export async function nodeRuntime(root, staging, platform) {
  const version = (await readFile(join(root, '.node-version'), 'utf8')).trim();
  assert.equal(process.version, `v${version}`, 'Build with the pinned Node release');
  const name = `node-v${version}-${platform}.tar.gz`;
  const pins = (await readFile(join(root, 'ops/distribution/node-checksums.txt'), 'utf8')).trim().split('\n').map(line => line.trim().split(/\s+/));
  const expected = pins.find(([, file]) => file === name)?.[0];
  assert.match(expected ?? '', /^[a-f0-9]{64}$/, 'No pinned Node archive for this platform');
  const cache = join(root, 'target/distribution-downloads');await mkdir(cache, { recursive: true, mode: 0o700 });
  const archive = join(cache, name);
  try { await access(archive); }
  catch {
    const partial = `${archive}.${process.pid}.partial`;
    execFileSync('curl', ['--fail', '--location', '--proto', '=https', '--tlsv1.2', '--retry', '2', '--max-time', '180', '--output', partial, `https://nodejs.org/dist/v${version}/${name}`], { stdio: 'inherit' });
    assert.equal(await fileHash(partial), expected, 'Downloaded Node checksum mismatch; partial file retained');
    await rename(partial, archive);
  }
  assert.equal(await fileHash(archive), expected, 'Cached Node checksum mismatch');
  const destination = join(staging, 'upstream-node');await mkdir(destination);
  execFileSync('tar', ['-xzf', archive, '--strip-components=1', '-C', destination], { stdio: 'inherit' });
  assert.equal(execFileSync(join(destination, 'bin/node'), ['--version'], { encoding: 'utf8' }).trim(), `v${version}`);
  return { root: destination, version, archive: name, sha256: expected };
}

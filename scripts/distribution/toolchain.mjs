import assert from 'node:assert/strict';
import { access, realpath, readFile } from 'node:fs/promises';
import { constants } from 'node:fs';
import { dirname, join, delimiter } from 'node:path';
import { fileHash } from './files.mjs';

export async function cargoTool(name, version) {
  let binary;
  for (const directory of (process.env.PATH ?? '').split(delimiter)) {
    const candidate = join(directory, name);
    try { await access(candidate, constants.X_OK);binary = await realpath(candidate);break; } catch { /* Continue PATH lookup. */ }
  }
  assert.ok(binary, `Install ${name} ${version} with cargo install --locked`);
  // cargo-auditable forwards --version to Cargo. Check Cargo's installation
  // receipt beside the actual executable and record the executable's hash.
  const receipt = JSON.parse(await readFile(join(dirname(dirname(binary)), '.crates2.json'), 'utf8'));
  const matching = Object.entries(receipt.installs).filter(([id, data]) => id.startsWith(`${name} ${version} (registry+`) && data.bins.includes(name));
  assert.equal(matching.length, 1, `The resolved ${name} must be installed at pinned version ${version}`);
  return { version, sha256: await fileHash(binary) };
}

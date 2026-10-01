import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileHash, json } from './files.mjs';

// Syft's CycloneDX file components can retain absolute executable names even
// with --base-path. Preserve their identities and hashes, but make names portable.
export async function portableSbom(input, output, bundleRoot) {
  const report = JSON.parse(await readFile(input, 'utf8'));
  assert.equal(report.bomFormat, 'CycloneDX');
  let normalized = 0;
  for (const component of report.components ?? []) {
    if (component.type !== 'file' || !component.name.startsWith(`${bundleRoot}/`)) continue;
    const relative = component.name.slice(bundleRoot.length + 1);
    assert.ok(relative.split('/').every(part => part && part !== '.' && part !== '..'), 'Unsafe SBOM file component');
    const expected = component.hashes?.find(hash => hash.alg === 'SHA-256')?.content;
    assert.equal(await fileHash(join(bundleRoot, relative)), expected, 'SBOM file digest does not match its bundle input');
    component.name = relative;normalized += 1;
  }
  await json(output, report);
  return { generator: 'scripts/distribution/sbom.mjs', normalization: 'verified file component names relative to bundle root; package facts, hashes and references preserved', normalized_file_names: normalized, syft_output_sha256: await fileHash(input), distributed_sbom_sha256: await fileHash(output) };
}

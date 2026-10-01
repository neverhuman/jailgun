import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileHash, json } from './files.mjs';
import { portableSbom } from './sbom.mjs';

test('portable SBOM preserves dependency facts and references, validates renamed file digests', async () => {
  const parent = resolve('target/distribution-verifier-proofs');await mkdir(parent, { recursive: true });
  const root = await mkdtemp(join(parent, 'sbom-'));await writeFile(join(root, 'node'), 'synthetic binary');
  const packageFact = { name: 'synthetic-dependency', version: '1.2.3', type: 'library', 'bom-ref': 'pkg:npm/synthetic-dependency@1.2.3' };
  const report = { bomFormat: 'CycloneDX', components: [{ name: join(root, 'node'), type: 'file', 'bom-ref': 'binary-id', hashes: [{ alg: 'SHA-256', content: await fileHash(join(root, 'node')) }] }, packageFact], dependencies: [{ ref: 'binary-id', dependsOn: [packageFact['bom-ref']] }] };
  const input = join(root, 'raw.json');const output = join(root, 'portable.json');await json(input, report);
  const receipt = await portableSbom(input, output, root);
  const portable = JSON.parse(await readFile(output, 'utf8'));
  assert.equal(receipt.normalized_file_names, 1);assert.equal(portable.components[0].name, 'node');
  assert.deepEqual(portable.components[1], packageFact);assert.deepEqual(portable.dependencies, report.dependencies);
  await writeFile(join(root, 'node'), 'altered');
  await assert.rejects(portableSbom(input, output, root), /does not match/);
});

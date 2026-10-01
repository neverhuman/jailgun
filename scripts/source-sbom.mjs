import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { randomUUID } from 'node:crypto';
import { lstat, mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { fileHash, hash, json } from './distribution/files.mjs';

const execute = promisify(execFile);
async function git(root, args) {
  return (await execute('git', args, { cwd: root, timeout: 10_000, maxBuffer: 32 * 1024 * 1024 })).stdout;
}
export async function sourceSnapshot(root, destination) {
  const paths = [...new Set((await git(root, ['ls-files', '--cached', '--others', '--exclude-standard', '-z'])).split('\0').filter(Boolean))].sort();
  assert.ok(paths.length > 0 && paths.length < 100_000, 'Invalid source file inventory');
  const source = { commit: (await git(root, ['rev-parse', 'HEAD'])).trim(), tree: (await git(root, ['rev-parse', 'HEAD^{tree}'])).trim(), dirty: Boolean((await git(root, ['status', '--porcelain'])).trim()), input_sha256: {} };
  await mkdir(destination, { recursive: false, mode: 0o700 });
  for (const path of paths) {
    assert.ok(!path.startsWith('/') && path.split('/').every(part => part && part !== '.' && part !== '..'), 'Invalid Git source path');
    assert.ok((await lstat(join(root, path))).isFile(), 'Source SBOM inputs must be regular files; links are not followed');
    const bytes = await readFile(join(root, path));source.input_sha256[path] = hash(bytes);
    await mkdir(dirname(join(destination, path)), { recursive: true, mode: 0o700 });
    await writeFile(join(destination, path), bytes, { flag: 'wx', mode: 0o600 });
  }
  return source;
}

async function run() {
  const root = resolve('.');
  const output = join(root, 'target/jankurai/security');await mkdir(output, { recursive: true });
  const staging = join(root, 'target/security-source-sbom', randomUUID());await mkdir(staging, { recursive: true, mode: 0o700 });
  const destination = join(staging, 'source');
  const source = await sourceSnapshot(root, destination);
  const pkg = JSON.parse(await readFile(join(destination, 'package.json'), 'utf8'));
  const tools = JSON.parse(await readFile(join(destination, 'ops/distribution/tools.json'), 'utf8'));
  const version = JSON.parse((await execute('syft', ['version', '-o', 'json'], { timeout: 10_000 })).stdout).version;
  assert.equal(version, tools.syft, 'Install the pinned Syft version');
  const sbom = join(output, 'sbom.spdx.json');
  await execute('syft', ['dir:.', '--base-path', '.', '--source-name', 'jailgun-source', '--source-version', pkg.version, '-o', `spdx-json=${sbom}`], { cwd: destination, timeout: 300_000, maxBuffer: 8 * 1024 * 1024 });
  const report = JSON.parse(await readFile(sbom, 'utf8'));
  for (const file of report.files ?? []) {
    const path = file.fileName.replace(/^\.\//, '');
    assert.ok(Object.hasOwn(source.input_sha256, path), 'SBOM referenced a file outside the source inventory');
  }
  assert.ok(!JSON.stringify(report).includes(staging), 'SBOM contains a private staging path');
  await json(join(output, 'sbom-evidence.json'), { generated_at: new Date().toISOString(), command: 'node scripts/source-sbom.mjs', exit_status: 0, syft: version, scope: 'Git-selected source and lockfiles; distribution dependencies are inventoried separately from the actual bundle', source, artifact_sha256: await fileHash(sbom) });
  console.log(`Source SBOM verified: ${Object.keys(source.input_sha256).length} source files; ${report.packages.length} package records`);
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) await run();

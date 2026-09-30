#!/usr/bin/env node
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { mkdir, readFile, writeFile, utimes } from 'node:fs/promises';
import { createReadStream, createWriteStream } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { pipeline } from 'node:stream/promises';
import { createGzip } from 'node:zlib';
import { copy, files, fileHash, json } from './distribution/files.mjs';
import { nodeRuntime } from './distribution/node-runtime.mjs';
import { notices } from './distribution/notices.mjs';
import { verifyBundle } from './distribution/verify.mjs';
import { portableSbom } from './distribution/sbom.mjs';
import { cargoTool } from './distribution/toolchain.mjs';
import { inspectSigning } from './distribution/signing.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
process.chdir(root);
const args = process.argv.slice(2);
const allowDirty = args.includes('--allow-dirty');
const outAt = args.indexOf('--out');
assert.ok(outAt < 0 || args[outAt + 1] && !args[outAt + 1].startsWith('--'), '--out requires a directory');
assert.ok(args.every((arg, i) => arg === '--allow-dirty' || arg === '--out' || i === outAt + 1 && outAt >= 0), 'Usage: node scripts/build-bundle.mjs [--allow-dirty] [--out target/directory]');
const output = resolve(outAt >= 0 ? args[outAt + 1] : `target/distribution/candidate-${randomUUID()}`);
assert.ok(output.startsWith(`${root}/target/`), 'Build output must be inside this checkout’s target directory');
await mkdir(dirname(output), { recursive: true });
await mkdir(output, { recursive: false });
const platform = `${process.platform}-${process.arch}`;
const target = { 'linux-x64': 'x86_64-unknown-linux-gnu', 'darwin-arm64': 'aarch64-apple-darwin', 'darwin-x64': 'x86_64-apple-darwin' }[platform];
assert.ok(target, `Unverified distribution platform: ${platform}`);
const run = (command, argv, options = {}) => execFileSync(command, argv, { cwd: root, stdio: 'inherit', timeout: command === 'cargo' ? 900_000 : 300_000, ...options });
const capture = (command, argv) => run(command, argv, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'], maxBuffer: 32 * 1024 * 1024 }).trim();
assert.ok(capture('rustc', ['-vV']).includes(`host: ${target}`), 'Build on the native distribution runner');
assert.ok(!process.env.CARGO_TARGET_DIR, 'Each checkout must use its own target directory');
assert.ok(!process.env.RUSTFLAGS && !process.env.CARGO_ENCODED_RUSTFLAGS, 'Distribution builds set their own reproducible path-remapping flags');
const tools = JSON.parse(await readFile('ops/distribution/tools.json', 'utf8'));
const auditable = await cargoTool('cargo-auditable', tools.cargo_auditable);
assert.equal(JSON.parse(capture('syft', ['version', '-o', 'json'])).version, tools.syft, 'Install the pinned Syft tool');
const tracked = () => [...new Set(capture('git', ['ls-files', '--cached', '--others', '--exclude-standard', '-z']).split('\0').filter(Boolean))].sort();
const sourcePaths = tracked();
const source = { commit: capture('git', ['rev-parse', 'HEAD']), tree: capture('git', ['rev-parse', 'HEAD^{tree}']), dirty: Boolean(capture('git', ['status', '--porcelain'])), input_sha256: {} };
assert.ok(!source.dirty || allowDirty, 'Commit the candidate first, or mark this a development build with --allow-dirty');
for (const path of sourcePaths) source.input_sha256[path] = await fileHash(path);
const packageJson = JSON.parse(await readFile('package.json', 'utf8'));
const metadata = JSON.parse(capture('cargo', ['metadata', '--locked', '--format-version', '1', '--filter-platform', target]));
assert.ok(metadata.packages.filter(pkg => metadata.workspace_members.includes(pkg.id)).every(pkg => pkg.version === packageJson.version), 'Rust and Node release versions disagree');
const version = packageJson.version;
const epoch = Number(capture('git', ['show', '-s', '--format=%ct', 'HEAD']));
const stage = join(output, 'bundle');await mkdir(stage);
const assets = join(stage, 'lib/jailgun');
const upstream = await nodeRuntime(root, output, platform);
run('npm', ['ci', '--ignore-scripts']);
assert.equal(JSON.parse(await readFile('node_modules/esbuild/package.json', 'utf8')).version, tools.esbuild);
run('npm', ['run', 'typecheck']);
const { build } = await import('vite');
const nodePackages = new Set([join(root, 'node_modules/playwright-core')]);
await build({ root: join(root, 'apps/dashboard'), plugins: [{ name: 'distribution-notice-inputs', generateBundle() {
  for (const id of this.getModuleIds()) {
    const marker = '/node_modules/';const index = id.lastIndexOf(marker);if (index < 0) continue;
    const tail = id.slice(index + marker.length);const packageName = tail.startsWith('@') ? tail.split('/').slice(0, 2).join('/') : tail.split('/')[0];
    nodePackages.add(id.slice(0, index + marker.length).replace(/^\0/, '') + packageName);
  }
} }] });
const playwright = JSON.parse(await readFile(join(root, 'node_modules/playwright-core/package.json'), 'utf8'));
assert.deepEqual(Object.keys(playwright.dependencies ?? {}), [], 'Review new bridge production dependencies before packaging');
const { build: compile } = await import('esbuild');
await compile({ entryPoints: ['scripts/distribution/verify-cli.mjs'], outfile: join(assets, 'verify-bundle.mjs'), bundle: true, platform: 'node', target: 'node24', format: 'esm' });
await compile({ entryPoints: ['scripts/distribution/install-cli.mjs'], outfile: join(assets, 'install-bundle.mjs'), bundle: true, platform: 'node', target: 'node24', format: 'esm' });
await copy(join(root, 'scripts/install.sh'), join(stage, 'install.sh'));
const bridge = await compile({ entryPoints: ['apps/chrome-bridge/bin/chrome-bridge.mjs', 'apps/chrome-bridge/bin/concept-bridge.mjs'], outdir: join(assets, 'apps/chrome-bridge/bin'), bundle: true, packages: 'external', platform: 'node', target: 'node24', format: 'esm', outExtension: { '.js': '.mjs' }, metafile: true });
for (const output of Object.values(bridge.metafile.outputs)) for (const dependency of output.imports) {
  assert.ok(dependency.path.startsWith('node:') || dependency.path === 'playwright-core', `Unpackaged bridge dependency: ${dependency.path}`);
}
await copy(join(root, 'node_modules/playwright-core'), join(assets, 'node_modules/playwright-core'));
await copy(join(root, 'apps/dashboard/dist'), join(assets, 'apps/dashboard/dist'));
await copy(join(upstream.root, 'bin/node'), join(assets, 'node/bin/node'));
await copy(join(upstream.root, 'LICENSE'), join(assets, 'node/LICENSE'));
await copy(join(root, 'LICENSE'), join(stage, 'LICENSE'));
await copy(join(root, 'config/jailgun.example.toml'), join(assets, 'config/jailgun.example.toml'));
await notices(root, stage, metadata, nodePackages);
const cargoHome = process.env.CARGO_HOME || join(process.env.HOME, '.cargo');
run('cargo', ['auditable', 'build', '--locked', '--release', '--target', target, '-p', 'jailgun-cli', '--bins'], { env: { ...process.env, CARGO_INCREMENTAL: process.env.CARGO_INCREMENTAL || '0', CARGO_PROFILE_DEV_DEBUG: process.env.CARGO_PROFILE_DEV_DEBUG || 'line-tables-only', CARGO_PROFILE_RELEASE_DEBUG: '0', CARGO_ENCODED_RUSTFLAGS: [`--remap-path-prefix=${root}=.`, `--remap-path-prefix=${cargoHome}=cargo`].join('\x1f') } });
for (const name of ['jailgun', 'jailhard']) await copy(join(root, 'target', target, 'release', name), join(stage, 'bin', name));
assert.equal(capture(join(stage, 'bin/jailgun'), ['--version']), `jailgun ${version}`);
const rawSbom = join(output, 'syft-raw.cdx.json');
run('syft', ['dir:.', '--base-path', '.', '--select-catalogers', '+javascript-package-cataloger', '--source-name', 'jailgun', '--source-version', version, '-o', `cyclonedx-json=${rawSbom}`], { cwd: stage });
const sbom = await portableSbom(rawSbom, join(stage, 'share/jailgun/sbom.cdx.json'), stage);
const catalogued = JSON.parse(await readFile(rawSbom, 'utf8')).components;
const inventory = JSON.parse(await readFile(join(stage, 'share/jailgun/notices/index.json'), 'utf8')).packages;
for (const dependency of inventory.filter(pkg => pkg.ecosystem === 'npm')) {
  assert.ok(catalogued.some(component => component.name === dependency.name && component.version === dependency.version), `SBOM missed bundled JavaScript dependency ${dependency.name}`);
}
for (const name of ['jailgun-cli', 'jailgun-core', 'jailgun-workflow', 'rusqlite', 'rmcp']) assert.ok(catalogued.some(component => component.name === name), `SBOM missed compiled Rust dependency ${name}`);
await json(join(stage, 'share/jailgun/sbom-generation.json'), sbom);
await json(join(stage, 'bundle.json'), { schema_version: 1, usage_lock_version: 1, version, platform, rust_target: target, source, tools: { ...tools, cargo_auditable: auditable }, node: { version: upstream.version, archive: upstream.archive, sha256: upstream.sha256 }, signing: inspectSigning(stage) });
await json(join(stage, 'manifest.json'), { schema_version: 1, files: await files(stage) });
await verifyBundle(stage);
assert.deepEqual(tracked(), sourcePaths, 'Source file set changed during build');
for (const [path, digest] of Object.entries(source.input_sha256)) assert.equal(await fileHash(path), digest, `Source changed during build: ${path}`);
const entries = await files(stage);
for (const file of entries) {
  const contents = await readFile(join(stage, file.path));
  assert.ok(!contents.includes(Buffer.from(root)), `Build checkout path leaked into ${file.path}`);
  assert.ok(!contents.includes(Buffer.from(cargoHome)), `Cargo home path leaked into ${file.path}`);
}
for (const file of entries) await utimes(join(stage, file.path), epoch, epoch);
const fileList = join(output, 'archive-files.txt');await writeFile(fileList, entries.map(file => `./${file.path}\n`).join(''));
const archive = join(output, `jailgun-${version}-${platform}.tar.gz`);
const tar = join(output, 'bundle.tar');
const flags = process.platform === 'linux' ? ['--owner=0', '--group=0', '--numeric-owner', '--no-recursion'] : ['--uid', '0', '--gid', '0', '--uname', '', '--gname', ''];
run('tar', ['--format=ustar', ...flags, '-cf', tar, '-C', stage, '-T', fileList], { env: { ...process.env, COPYFILE_DISABLE: '1' } });
await pipeline(createReadStream(tar), createGzip({ level: 9 }), createWriteStream(archive, { flags: 'wx' }));
await copy(join(stage, 'install.sh'), join(output, 'install.sh'));
await writeFile(join(output, 'SHA256SUMS'), `${await fileHash(archive)}  ${archive.split('/').at(-1)}\n${await fileHash(join(output, 'install.sh'))}  install.sh\n`);
await json(join(output, 'build-evidence.json'), { source, command: 'node scripts/build-bundle.mjs', exit_status: 0, node: process.version, rustc: capture('rustc', ['--version']), syft: capture('syft', ['version', '-o', 'json']), generated_at: new Date().toISOString(), archive: archive.split('/').at(-1), archive_sha256: await fileHash(archive), file_count: entries.length, runtime_verified: false, live_provider_verified: false });
console.log(`Bundle created: ${archive}\nThis build requires package and live acceptance before publication.`);

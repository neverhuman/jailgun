import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { randomUUID, createHash } from 'node:crypto';
import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { basename, dirname, join, resolve } from 'node:path';
import { promisify } from 'node:util';

const configUrl = new URL('../../../ops/distribution/clean-linux.json', import.meta.url);
const configBytes = await readFile(configUrl);
const config = JSON.parse(configBytes);
const image = config.image;
assert.match(image, /^ubuntu@sha256:[a-f0-9]{64}$/);
assert.equal(config.platform, 'linux/amd64');
const execute = promisify(execFile);
const command = (args, timeout = 30_000) => execute('docker', args, { timeout, maxBuffer: 4 * 1024 * 1024 });
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const archive = resolve(process.argv[2]);
const name = basename(archive);
assert.equal(process.platform, 'linux');
assert.match(name, /^jailgun-[0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9.-]+)?-linux-x64\.tar\.gz$/);
assert.ok(process.getuid() > 0, 'Run the clean installation proof as a non-root user');
const id = randomUUID();
const proof = resolve('target/clean-linux-proofs', id);
assert.ok(!proof.includes(','), 'Docker bind mounts require a checkout path without commas');
const download = join(proof, 'download');
const scripts = join(proof, 'scripts');
const evidence = join(proof, 'evidence');
for (const path of [download, scripts, evidence]) await mkdir(path, { recursive: true, mode: 0o700 });
const sums = (await readFile(join(dirname(archive), 'SHA256SUMS'), 'utf8')).trim().split('\n').map(line => line.trim().split(/\s+/));
for (const file of [name, 'install.sh']) {
  const matches = sums.filter(([, entry]) => entry === file);
  assert.equal(matches.length, 1);
  assert.equal(hash(await readFile(join(dirname(archive), file))), matches[0][0]);
  await copyFile(join(dirname(archive), file), join(download, file));
}
await copyFile(join(dirname(archive), 'SHA256SUMS'), join(download, 'SHA256SUMS'));
const toolInputs = [{ path: 'ops/distribution/clean-linux.json', sha256: hash(configBytes) }];
for (const file of ['entry.sh', 'smoke.mjs']) {
  const source = new URL(file, import.meta.url);
  await copyFile(source, join(scripts, file));
  toolInputs.push({ path: `scripts/distribution/clean/${file}`, sha256: hash(await readFile(source)) });
}
toolInputs.push({ path: 'scripts/distribution/clean/host.mjs', sha256: hash(await readFile(new URL(import.meta.url))) });
await command(['version', '--format', '{{.Server.Version}}']);
try { await command(['image', 'inspect', image]); }
catch { await command(['pull', '--platform=linux/amd64', image], 180_000); }
const metadata = JSON.parse((await command(['image', 'inspect', image])).stdout)[0];
assert.ok(metadata.RepoDigests.includes(image));
assert.equal(metadata.Os, 'linux');
assert.equal(metadata.Architecture, 'amd64');
const container = `jailgun-clean-${id}`;
let result;
try {
  result = await command(['run', '--rm', '--name', container, '--label', `io.jailgun.proof=${id}`,
    '--platform=linux/amd64', '--network=none', '--read-only', '--cap-drop=ALL', '--security-opt=no-new-privileges',
    '--pids-limit=512', '--cpus=2', '--memory=1g', '--user', `${process.getuid()}:${process.getgid()}`,
    '--tmpfs', `/home/test:rw,exec,uid=${process.getuid()},gid=${process.getgid()},mode=0700`,
    '--mount', `type=bind,source=${download},target=/download,readonly`,
    '--mount', `type=bind,source=${scripts},target=/proof,readonly`,
    '--mount', `type=bind,source=${evidence},target=/evidence`, image, 'bash', '/proof/entry.sh', name], 180_000);
} catch (error) {
  await writeFile(join(proof, 'command.log'), `${error.stdout ?? ''}\n${error.stderr ?? ''}`, { mode: 0o600 });
  throw error;
} finally {
  let observed;
  try { observed = JSON.parse((await command(['container', 'inspect', container])).stdout)[0]; }
  catch (error) { if (!error.stderr?.includes('No such container') && !error.stderr?.includes('No such object')) throw error; }
  if (observed) {
    assert.equal(observed.Config.Labels['io.jailgun.proof'], id, 'Refuse cleanup of a foreign container');
    await command(['container', 'rm', '--force', container]);
  }
}
await writeFile(join(proof, 'command.log'), `${result.stdout}\n${result.stderr}`, { mode: 0o600 });
const report = JSON.parse(await readFile(join(evidence, 'result.json'), 'utf8'));
assert.equal(report.status, 'pass');
assert.equal(report.platform, 'linux-x64');
assert.equal(report.os.match(/^VERSION_ID="([^"]+)"$/m)?.[1], config.ubuntu_version);
assert.match(report.source.commit, /^[a-f0-9]{40}$/);
const record = { generated_at: new Date().toISOString(), status: 'pass', command: 'node scripts/distribution/clean/host.mjs <verified-linux-archive>', exit_status: 0,
  image, image_id: metadata.Id, archive_sha256: hash(await readFile(archive)), tools: toolInputs, result: report };
await writeFile(join(proof, 'evidence.json'), `${JSON.stringify(record, null, 2)}\n`, { mode: 0o600 });
console.log(`Clean Ubuntu installation checks passed: ${join(proof, 'evidence.json')}`);

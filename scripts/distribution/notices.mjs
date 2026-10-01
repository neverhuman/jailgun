import assert from 'node:assert/strict';
import { readFile, readdir, lstat, writeFile } from 'node:fs/promises';
import { basename, dirname, join, resolve } from 'node:path';
import { copy, json } from './files.mjs';

async function licenseFiles(root) {
  const names = await readdir(root);
  return names.filter(name => /^(LICEN[CS]E|COPYING|NOTICE|UNLICENSE)([.-]|$)/i.test(name));
}
async function preserve(root, destination, extra = []) {
  const names = [...new Set([...await licenseFiles(root), ...extra])];
  for (const name of names) await copy(resolve(root, name), join(destination, basename(name)));
  return names.map(name => basename(name));
}

export async function notices(root, bundle, metadata, nodePackages) {
  const destination = join(bundle, 'share/jailgun/notices');
  const inventory = [];
  const byId = new Map(metadata.packages.map(pkg => [pkg.id, pkg]));
  const nodes = new Map(metadata.resolve.nodes.map(node => [node.id, node]));
  const pending = [metadata.packages.find(pkg => pkg.name === 'jailgun-cli').id];
  const visited = new Set();
  while (pending.length) {
    const id = pending.pop();if (visited.has(id)) continue;visited.add(id);
    for (const dep of nodes.get(id).deps) if (dep.dep_kinds.some(kind => kind.kind !== 'dev')) pending.push(dep.pkg);
    const pkg = byId.get(id);
    if (metadata.workspace_members.includes(id)) continue;
    const path = `rust/${pkg.name}-${pkg.version}`;
    const extra = pkg.license_file ? [pkg.license_file] : [];
    let licenses = await preserve(dirname(pkg.manifest_path), join(destination, path), extra);
    if (pkg.name === 'rmcp' && pkg.version === '3.4.0' && licenses.length === 0) {
      const name = 'LICENSE';await copy(join(root, 'ops/distribution/licenses/rmcp-LICENSE'), join(destination, path, name));licenses = [name];
    }
    assert.ok(licenses.length, `No license file for Rust dependency ${pkg.name} ${pkg.version}`);
    inventory.push({ ecosystem: 'cargo', name: pkg.name, version: pkg.version, license: pkg.license, repository: pkg.repository, notice_directory: path, files: licenses });
  }
  for (const packageRoot of [...nodePackages].sort()) {
    const pkg = JSON.parse(await readFile(join(packageRoot, 'package.json'), 'utf8'));
    const path = `npm/${pkg.name.replace('/', '__')}-${pkg.version}`;
    const licenses = await preserve(packageRoot, join(destination, path));
    assert.ok(licenses.length, `No license file for bundled npm dependency ${pkg.name} ${pkg.version}`);
    await copy(join(packageRoot, 'package.json'), join(destination, path, 'package.json'));
    inventory.push({ ecosystem: 'npm', name: pkg.name, version: pkg.version, license: pkg.license, repository: pkg.repository, notice_directory: path, files: licenses });
    if (pkg.name === '@novnc/novnc') {
      // Preserve the unmodified corresponding source and its nested license notices.
      assert.equal(pkg.version, '1.7.0');
      for (const name of ['core', 'vendor', 'docs', 'AUTHORS', 'LICENSE.txt', 'package.json']) {
        assert.ok((await lstat(join(packageRoot, name))).isFile() || (await lstat(join(packageRoot, name))).isDirectory());
        await copy(join(packageRoot, name), join(destination, 'source/novnc', name));
      }
    }
  }
  await writeFile(join(destination, 'README.md'), (await readFile(join(root, 'docs/third-party-notices.md'), 'utf8')).replace('(../LICENSE)', '(../../../LICENSE)'));
  await json(join(destination, 'index.json'), { scope: 'Rust runtime/build dependencies and bundled JavaScript packages; Node notices are in lib/jailgun/node/LICENSE', packages: inventory.sort((a, b) => `${a.ecosystem}:${a.name}`.localeCompare(`${b.ecosystem}:${b.name}`)) });
}

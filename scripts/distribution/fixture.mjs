import { mkdtemp, mkdir, writeFile, chmod } from 'node:fs/promises';
import { resolve, join, dirname } from 'node:path';
import { files, json } from './files.mjs';

export async function fixture() {
  const parent = resolve('target/distribution-verifier-proofs');await mkdir(parent, { recursive: true });
  const root = await mkdtemp(join(parent, 'bundle-'));
  for (const path of ['bin/jailgun', 'bin/jailhard', 'lib/jailgun/node/bin/node', 'lib/jailgun/node/LICENSE', 'lib/jailgun/apps/chrome-bridge/bin/chrome-bridge.mjs', 'lib/jailgun/apps/chrome-bridge/bin/concept-bridge.mjs', 'lib/jailgun/apps/dashboard/dist/index.html', 'lib/jailgun/config/jailgun.example.toml', 'lib/jailgun/install-bundle.mjs', 'share/jailgun/notices/index.json', 'share/jailgun/sbom.cdx.json', 'LICENSE', 'install.sh']) {
    await mkdir(dirname(join(root, path)), { recursive: true });await writeFile(join(root, path), 'Synthetic verification fixture');
    if (path.startsWith('bin/') || path.endsWith('/bin/node')) await chmod(join(root, path), 0o755);
  }
  await json(join(root, 'bundle.json'), { schema_version: 1, usage_lock_version: 1, version: '0.0.0-test', platform: `${process.platform}-${process.arch}`, node: { version: process.version.slice(1) }, source: { commit: '0'.repeat(40), tree: '1'.repeat(40), dirty: true } });
  await json(join(root, 'manifest.json'), { schema_version: 1, files: await files(root) });
  return root;
}

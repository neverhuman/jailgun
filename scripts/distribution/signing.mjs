import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { join } from 'node:path';

export function inspectSigning(root, platform = process.platform, execute = spawnSync) {
  const result = { notarization: 'not-performed' };
  for (const [name, path] of [['jailgun', 'bin/jailgun'], ['jailhard', 'bin/jailhard'], ['node', 'lib/jailgun/node/bin/node']]) {
    if (platform === 'linux') { result[name] = { status: 'unsigned', format: 'ELF' };continue; }
    assert.equal(platform, 'darwin', 'Unsupported signing inspection platform');
    const options = { encoding: 'utf8', timeout: 10_000, maxBuffer: 1024 * 1024 };
    const display = execute('/usr/bin/codesign', ['--display', '--verbose=4', join(root, path)], options);
    assert.ok(!display.error && display.signal == null, `Code signature inspection failed for ${name}`);
    const detail = `${display.stdout ?? ''}\n${display.stderr ?? ''}`;
    if (display.status !== 0) {
      assert.match(detail, /code object is not signed at all/, `Unrecognized code signature inspection failure for ${name}`);
      result[name] = { status: 'unsigned', format: 'Mach-O' };continue;
    }
    const verified = execute('/usr/bin/codesign', ['--verify', '--strict', join(root, path)], options);
    assert.ok(!verified.error && verified.status === 0 && verified.signal == null, `Invalid code signature for ${name}`);
    const authorities = [...detail.matchAll(/^Authority=(.+)$/gm)].map(match => match[1]);
    result[name] = {
      status: /^Signature=adhoc$/m.test(detail) ? 'ad-hoc' : 'signed', format: 'Mach-O', verified: true,
      identifier: detail.match(/^Identifier=(.+)$/m)?.[1] ?? null,
      team_identifier: detail.match(/^TeamIdentifier=(.+)$/m)?.[1] ?? null,
      authorities,
    };
  }
  return result;
}

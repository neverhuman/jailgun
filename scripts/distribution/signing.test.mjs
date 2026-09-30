import assert from 'node:assert/strict';
import test from 'node:test';
import { inspectSigning } from './signing.mjs';

test('distinguishes verified ad-hoc and vendor signatures without claiming notarization', () => {
  const record = inspectSigning('/synthetic bundle', 'darwin', (_, args) => {
    if (args[0] === '--verify') return { status: 0 };
    return { status: 0, stderr: args.at(-1).endsWith('/node')
      ? 'Identifier=node\nAuthority=Developer ID Application: Synthetic Vendor\nTeamIdentifier=SYNTHETIC\n'
      : 'Identifier=jailgun\nSignature=adhoc\nTeamIdentifier=not set\n' };
  });
  assert.equal(record.jailgun.status, 'ad-hoc');
  assert.equal(record.node.status, 'signed');
  assert.equal(record.notarization, 'not-performed');
});
test('signature failures cannot be misreported as an unsigned successful build', () => {
  assert.throws(() => inspectSigning('/synthetic', 'darwin', () => ({ status: 1, stderr: 'resource envelope is obsolete' })), /Unrecognized/);
  assert.throws(() => inspectSigning('/synthetic', 'darwin', (_, args) => args[0] === '--verify' ? { status: 1 } : { status: 0, stderr: 'Signature=adhoc\n' }), /Invalid code signature/);
  assert.throws(() => inspectSigning('/synthetic', 'darwin', () => ({ error: new Error('unavailable') })), /inspection failed/);
  assert.equal(inspectSigning('/synthetic', 'darwin', () => ({ status: 1, stderr: 'code object is not signed at all' })).jailhard.status, 'unsigned');
});

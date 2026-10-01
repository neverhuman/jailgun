import assert from 'node:assert/strict';
import { existsSync, lstatSync, mkdtempSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { clearStaleProfileLocks, managedChromeArgs, managedChromeCandidates, resolveManagedChromeExecutable } from '../src/managed-chrome.mjs';

test('managed Chrome uses a normal launch with private loopback debugging', () => {
  const args = managedChromeArgs({ profileDir: '/private/account', port: 9444 });
  assert.ok(args.includes('--user-data-dir=/private/account'));
  assert.ok(args.includes('--remote-debugging-address=127.0.0.1'));
  assert.ok(args.includes('--remote-debugging-port=9444'));
  assert.ok(!args.includes('--enable-automation'));
  assert.ok(!args.includes('--remote-debugging-pipe'));
});

test('managed Chrome rejects unsafe ports', () => {
  assert.throws(() => managedChromeArgs({ profileDir: '/private/account', port: 0 }), /config-invalid/);
  assert.throws(() => managedChromeArgs({ profileDir: '/private/account', port: 70000 }), /config-invalid/);
});

test('headless mode is explicit and reserved for loopback fixtures by the caller', () => {
  const args = managedChromeArgs({ profileDir: '/private/fixture', port: 9445, headless: true });
  assert.ok(args.includes('--headless=new'));
});

test('Chrome discovery is deterministic when the service omits an executable', () => {
  assert.equal(resolveManagedChromeExecutable('/custom/chrome'), '/custom/chrome');
  assert.ok(managedChromeCandidates('linux').includes('/opt/google/chrome/chrome'));
  assert.ok(managedChromeCandidates('darwin').includes('/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'));
});

test('stale profile locks are removed but live locks are preserved', () => {
  const stale = mkdtempSync(join(tmpdir(), 'jailgun-stale-profile-'));
  symlinkSync('host-424242', join(stale, 'SingletonLock'));
  symlinkSync('/tmp/stale-socket', join(stale, 'SingletonSocket'));
  writeFileSync(join(stale, 'SingletonCookie'), 'stale');
  assert.equal(clearStaleProfileLocks(stale, () => false), true);
  assert.equal(existsSync(join(stale, 'SingletonLock')), false);
  assert.equal(existsSync(join(stale, 'SingletonSocket')), false);
  assert.equal(existsSync(join(stale, 'SingletonCookie')), false);

  const live = mkdtempSync(join(tmpdir(), 'jailgun-live-profile-'));
  symlinkSync('host-31337', join(live, 'SingletonLock'));
  assert.equal(clearStaleProfileLocks(live, () => true), false);
  assert.equal(lstatSync(join(live, 'SingletonLock')).isSymbolicLink(), true);
});

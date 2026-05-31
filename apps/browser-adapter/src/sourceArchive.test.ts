import { mkdir, mkdtemp, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { describe, expect, it } from 'vitest';

import { DEFAULT_SOURCE_ARCHIVE_TMP_PARENT, cleanupSourceArchive, createTempSourceArchive } from './sourceArchive';

it('creates a git archive in a temp directory and cleans it up', async () => {
  const fixtureRoot = await mkdtemp(join(tmpdir(), 'jailgun-git-fixture-'));
  let archive: Awaited<ReturnType<typeof createTempSourceArchive>> | null = null;
  try {
    spawnSync('git', ['init'], { cwd: fixtureRoot, stdio: 'ignore' });
    spawnSync('git', ['config', 'user.email', 'test@example.invalid'], { cwd: fixtureRoot, stdio: 'ignore' });
    spawnSync('git', ['config', 'user.name', 'Test User'], { cwd: fixtureRoot, stdio: 'ignore' });
    await mkdir(join(fixtureRoot, 'src'), { recursive: true });
    await mkdir(join(fixtureRoot, 'assets'), { recursive: true });
    await writeFile(join(fixtureRoot, 'README.md'), 'fixture\n');
    await writeFile(join(fixtureRoot, 'src', 'lib.rs'), 'fn main() {}\n');
    await writeFile(join(fixtureRoot, 'package.json'), '{"type":"module"}\n');
    await writeFile(join(fixtureRoot, 'package-lock.json'), '{"lockfileVersion":3}\n');
    await writeFile(join(fixtureRoot, 'assets', 'logo.png'), Buffer.from([0x89, 0x50, 0x4e, 0x47]));
    spawnSync('git', ['add', '.'], { cwd: fixtureRoot, stdio: 'ignore' });
    spawnSync('git', ['commit', '-m', 'fixture'], { cwd: fixtureRoot, stdio: 'ignore' });

    archive = await createTempSourceArchive({
      repoUrl: fixtureRoot,
      prefix: 'source/',
      archiveFilename: 'source.tar.gz'
    });
    const archiveStat = await stat(archive.archivePath);
    expect(archive.tempRoot.startsWith(`${DEFAULT_SOURCE_ARCHIVE_TMP_PARENT}/jailgun-source-`)).toBe(true);
    expect(archiveStat.size).toBeGreaterThan(0);
    expect(archive.commit).toMatch(/^[a-f0-9]{40}$/);
    expect(archiveEntries(archive.archivePath)).toEqual([
      'source/',
      'source/README.md',
      'source/package.json',
      'source/src/',
      'source/src/lib.rs'
    ]);

    await cleanupSourceArchive(archive);
    await expect(stat(archive.tempRoot)).rejects.toThrow();
  } finally {
    if (archive) {
      await cleanupSourceArchive(archive).catch(() => undefined);
    }
    await rm(fixtureRoot, { recursive: true, force: true });
  }
});

it('can opt into full archives for debugging', async () => {
  const fixtureRoot = await mkdtemp(join(tmpdir(), 'jailgun-git-fixture-'));
  let archive: Awaited<ReturnType<typeof createTempSourceArchive>> | null = null;
  try {
    spawnSync('git', ['init'], { cwd: fixtureRoot, stdio: 'ignore' });
    spawnSync('git', ['config', 'user.email', 'test@example.invalid'], { cwd: fixtureRoot, stdio: 'ignore' });
    spawnSync('git', ['config', 'user.name', 'Test User'], { cwd: fixtureRoot, stdio: 'ignore' });
    await mkdir(join(fixtureRoot, 'assets'), { recursive: true });
    await writeFile(join(fixtureRoot, 'README.md'), 'fixture\n');
    await writeFile(join(fixtureRoot, 'assets', 'logo.png'), Buffer.from([0x89, 0x50, 0x4e, 0x47]));
    spawnSync('git', ['add', '.'], { cwd: fixtureRoot, stdio: 'ignore' });
    spawnSync('git', ['commit', '-m', 'fixture'], { cwd: fixtureRoot, stdio: 'ignore' });

    archive = await createTempSourceArchive({
      repoUrl: fixtureRoot,
      prefix: 'source/',
      archiveFilename: 'source.tar.gz',
      mode: 'full'
    });
    expect(archiveEntries(archive.archivePath)).toContain('source/assets/logo.png');
  } finally {
    if (archive) {
      await cleanupSourceArchive(archive).catch(() => undefined);
    }
    await rm(fixtureRoot, { recursive: true, force: true });
  }
});

describe('source archive option validation', () => {
  it('rejects unsafe archive filenames', async () => {
    await expect(
      createTempSourceArchive({
        repoUrl: 'https://example.invalid/repo.git',
        prefix: 'source/',
        archiveFilename: '../source.tar.gz'
      })
    ).rejects.toThrow(/safe basename/);
  });

  it('rejects relative temp parents so archives cannot land in the repo', async () => {
    await expect(
      createTempSourceArchive({
        repoUrl: 'https://example.invalid/repo.git',
        prefix: 'source/',
        archiveFilename: 'source.tar.gz',
        tmpParent: 'relative-tmp'
      })
    ).rejects.toThrow(/absolute path/);
  });
});

function archiveEntries(archivePath: string): string[] {
  const result = spawnSync('tar', ['-tzf', archivePath], { encoding: 'utf8' });
  if (result.status !== 0) {
    throw new Error(result.stderr);
  }
  return result.stdout
    .trim()
    .split('\n')
    .filter((entry) => entry !== 'pax_global_header')
    .sort();
}

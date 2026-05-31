import { stat } from 'node:fs/promises';

import { expect, it } from 'vitest';

import { DEFAULT_SOURCE_ARCHIVE_TMP_PARENT, cleanupSourceArchive, createTempSourceArchive } from './sourceArchive';
import { archiveEntries, withGitFixture } from './sourceArchive.testSupport';

it('creates a git archive in a temp directory and cleans it up', async () => {
  let archive: Awaited<ReturnType<typeof createTempSourceArchive>> | null = null;
  await withGitFixture(
    [
      { path: 'README.md', contents: 'fixture\n' },
      { path: 'src/lib.rs', contents: 'fn main() {}\n' },
      { path: 'package.json', contents: '{"type":"module"}\n' },
      { path: 'package-lock.json', contents: '{"lockfileVersion":3}\n' },
      { path: 'assets/logo.png', contents: Buffer.from([0x89, 0x50, 0x4e, 0x47]) }
    ],
    async (fixtureRoot) => {
      try {
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
      }
    }
  );
});

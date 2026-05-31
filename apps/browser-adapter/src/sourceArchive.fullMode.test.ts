import { expect, it } from 'vitest';

import { cleanupSourceArchive, createTempSourceArchive } from './sourceArchive';
import { archiveEntries, withGitFixture } from './sourceArchive.testSupport';

it('can opt into full archives for debugging', async () => {
  let archive: Awaited<ReturnType<typeof createTempSourceArchive>> | null = null;
  await withGitFixture(
    [
      { path: 'README.md', contents: 'fixture\n' },
      { path: 'assets/logo.png', contents: Buffer.from([0x89, 0x50, 0x4e, 0x47]) }
    ],
    async (fixtureRoot) => {
      try {
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
      }
    }
  );
});

import { describe, expect, it } from 'vitest';

import { createTempSourceArchive } from './sourceArchive';

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

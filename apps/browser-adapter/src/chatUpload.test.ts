import { mkdtemp, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import { uploadSourceArchiveThenSubmitPrompt, type PageLike } from './chatUpload';

describe('uploadSourceArchiveThenSubmitPrompt', () => {
  it('uploads, confirms, deletes temp files, then submits prompt', async () => {
    const tempRoot = await mkdtemp(join(tmpdir(), 'jailgun-upload-fixture-'));
    const archivePath = join(tempRoot, 'source.tar.gz');
    await writeFile(archivePath, 'archive');
    const events: string[] = [];
    const page = fakePage(events);

    const result = await uploadSourceArchiveThenSubmitPrompt({
      page,
      prompt: 'Use the uploaded source archive.',
      archive: {
        repoUrl: 'unused',
        prefix: 'source/',
        archiveFilename: 'source.tar.gz'
      },
      archiveFactory: async () => ({
        tempRoot,
        cloneDir: join(tempRoot, 'repo'),
        archivePath,
        archiveFilename: 'source.tar.gz',
        commit: 'a'.repeat(40)
      }),
      uploadFile: async () => {
        events.push('upload');
      },
      confirmUpload: async () => {
        events.push('confirm');
      },
      submitPrompt: async (_page, prompt) => {
        await expect(stat(tempRoot)).rejects.toThrow();
        events.push(`prompt:${prompt}`);
      }
    });

    expect(result.deletedBeforePrompt).toBe(true);
    expect(events).toEqual(['upload', 'confirm', 'prompt:Use the uploaded source archive.']);
  });

  it('does not submit the prompt when cleanup fails to delete temp files', async () => {
    const tempRoot = await mkdtemp(join(tmpdir(), 'jailgun-upload-fixture-'));
    const archivePath = join(tempRoot, 'source.tar.gz');
    await writeFile(archivePath, 'archive');
    const events: string[] = [];

    try {
      await expect(
        uploadSourceArchiveThenSubmitPrompt({
          page: fakePage(events),
          prompt: 'This must not be submitted.',
          archive: {
            repoUrl: 'unused',
            prefix: 'source/',
            archiveFilename: 'source.tar.gz'
          },
          archiveFactory: async () => ({
            tempRoot,
            cloneDir: join(tempRoot, 'repo'),
            archivePath,
            archiveFilename: 'source.tar.gz',
            commit: 'b'.repeat(40)
          }),
          uploadFile: async () => {
            events.push('upload');
          },
          confirmUpload: async () => {
            events.push('confirm');
          },
          archiveCleanup: async () => {
            events.push('cleanup');
          },
          submitPrompt: async () => {
            events.push('prompt');
          }
        })
      ).rejects.toThrow(/still exists after cleanup/);
      expect(events).toEqual(['upload', 'confirm', 'cleanup', 'cleanup']);
    } finally {
      await rm(tempRoot, { recursive: true, force: true });
    }
  });
});

function fakePage(events: string[]): PageLike {
  return {
    locator: () => ({
      count: async () => 1,
      first() {
        return this;
      },
      setInputFiles: async () => {
        events.push('set-files');
      },
      fill: async (text: string) => {
        events.push(`fill:${text}`);
      },
      click: async () => {
        events.push('click');
      }
    })
  };
}

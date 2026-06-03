import { mkdtemp, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import {
  PromptSubmitReadinessError,
  submitPromptToChat,
  uploadSourceArchiveThenSubmitPrompt,
  type LocatorLike,
  type PageLike
} from './chatUpload';

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

describe('submitPromptToChat readiness', () => {
  it('clicks send only after delayed upload readiness enables the button', async () => {
    const events: string[] = [];
    let enabledChecks = 0;
    const page = readinessPage({
      events,
      sendEnabled: async () => {
        enabledChecks += 1;
        return enabledChecks >= 3;
      },
      waitForTimeout: async () => undefined
    });

    await submitPromptToChat(page, 'Apply the uploaded source.', 1_000);

    expect(events).toEqual(['fill:Apply the uploaded source.', 'click-send']);
    expect(enabledChecks).toBeGreaterThanOrEqual(3);
  });

  it('times out during a long upload and reports the last disabled upload state', async () => {
    const events: string[] = [];
    const page = readinessPage({
      events,
      sendEnabled: async () => false,
      sendLabel: 'Uploading source.tar.gz'
    });

    const error = await submitPromptToChat(page, 'Wait for upload.', 20).catch((caught: unknown) => caught);

    expect(error).toBeInstanceOf(PromptSubmitReadinessError);
    expect((error as PromptSubmitReadinessError).lastObserved?.enabled).toBe(false);
    expect((error as PromptSubmitReadinessError).lastObserved?.uploadState).toContain('Uploading');
    expect(events).toEqual(['fill:Wait for upload.']);
  });

  it('does not click send when the prompt disappears before readiness', async () => {
    const events: string[] = [];
    let readCount = 0;
    const page = readinessPage({
      events,
      sendEnabled: async () => true,
      composerText: () => {
        readCount += 1;
        return readCount === 1 ? 'Prompt that vanishes.' : '';
      }
    });

    const error = await submitPromptToChat(page, 'Prompt that vanishes.', 1_000).catch(
      (caught: unknown) => caught
    );

    expect(error).toBeInstanceOf(PromptSubmitReadinessError);
    expect(String((error as Error).message)).toContain('composer text disappeared');
    expect(events).toEqual(['fill:Prompt that vanishes.']);
  });

  it('never clicks send or presses Enter while the send button is disabled', async () => {
    const events: string[] = [];
    const page = readinessPage({
      events,
      sendEnabled: async () => false,
      sendLabel: 'Send disabled while upload finishes'
    });

    await expect(submitPromptToChat(page, 'No premature submit.', 20)).rejects.toThrow(
      PromptSubmitReadinessError
    );

    expect(events).toEqual(['fill:No premature submit.']);
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

interface ReadinessPageOptions {
  events: string[];
  sendEnabled: () => Promise<boolean>;
  sendLabel?: string;
  composerText?: () => string;
  waitForTimeout?: (ms: number) => Promise<void>;
}

function readinessPage(options: ReadinessPageOptions): PageLike {
  let composerValue = '';
  const composer: LocatorLike = {
    count: async () => 1,
    first() {
      return this;
    },
    fill: async (text: string) => {
      composerValue = text;
      options.events.push(`fill:${text}`);
    },
    inputValue: async () => options.composerText?.() ?? composerValue,
    press: async (key: string) => {
      options.events.push(`press:${key}`);
    }
  };
  const send: LocatorLike = {
    count: async () => 1,
    first() {
      return this;
    },
    nth() {
      return this;
    },
    isVisible: async () => true,
    isEnabled: options.sendEnabled,
    getAttribute: async (name: string) => {
      if (name === 'aria-label') {
        return options.sendLabel ?? 'Send message';
      }
      if (name === 'aria-disabled') {
        return (await options.sendEnabled()) ? 'false' : 'true';
      }
      return null;
    },
    textContent: async () => options.sendLabel ?? 'Send',
    click: async () => {
      options.events.push('click-send');
    }
  };
  return {
    locator: (selector: string) => {
      if (selector === '#prompt-textarea') {
        return composer;
      }
      return send;
    },
    waitForTimeout: options.waitForTimeout
  };
}

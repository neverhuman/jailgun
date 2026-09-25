import { existsSync } from 'node:fs';

import {
  cleanupSourceArchive,
  createTempSourceArchive
} from '../sourceArchive';
import type { SourceArchiveResult } from '../sourceArchive';
import type { PageLike, UploadArchivePromptOptions, UploadArchivePromptResult } from './types';
import { assertTempRootDeleted } from './utils';
import { defaultConfirmUpload, uploadFileToChat } from './upload';
import { submitPromptToChat } from './promptSubmit';

export async function uploadSourceArchiveThenSubmitPrompt(
  options: UploadArchivePromptOptions
): Promise<UploadArchivePromptResult> {
  const timeoutMs = options.timeoutMs ?? 45_000;
  const archive = await (options.archiveFactory ?? (() => createTempSourceArchive(options.archive)))();
  const cleanup = options.archiveCleanup ?? cleanupSourceArchive;
  const confirmUpload =
    options.confirmUpload ??
    ((page: PageLike, uploadedArchive: SourceArchiveResult, timeout: number) =>
      defaultConfirmUpload(page, uploadedArchive, timeout, options.confirmationSelectors));
  let uploadConfirmed = false;

  try {
    await (options.uploadFile ?? uploadFileToChat)(options.page, archive.archivePath, timeoutMs);
    await confirmUpload(options.page, archive, timeoutMs);
    uploadConfirmed = true;
    await cleanup(archive);
    assertTempRootDeleted(archive.tempRoot);
    await (options.submitPrompt ?? submitPromptToChat)(options.page, options.prompt, timeoutMs);
    return {
      archivePath: archive.archivePath,
      archiveFilename: archive.archiveFilename,
      commit: archive.commit,
      deletedBeforePrompt: true
    };
  } finally {
    if (!uploadConfirmed || existsSync(archive.tempRoot)) {
      await cleanup(archive).catch(() => undefined);
    }
  }
}

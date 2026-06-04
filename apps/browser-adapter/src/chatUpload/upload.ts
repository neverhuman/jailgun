import { basename } from 'node:path';

import type { SourceArchiveResult } from '../sourceArchive';
import type { PageLike } from './types';
import { MissingChatControlError } from './errors';
import { cssAttr, first, firstAvailableLocator } from './utils';

export async function uploadFileToChat(page: PageLike, archivePath: string, timeoutMs = 45_000): Promise<void> {
  const input = first(page.locator('input[type="file"]'));
  const inputCount = await input.count?.().catch(() => 0);
  if ((inputCount ?? 0) > 0 && input.setInputFiles) {
    await input.setInputFiles(archivePath);
    return;
  }

  if (!page.waitForEvent) {
    throw new Error('page does not support file chooser upload');
  }
  const chooserPromise = page.waitForEvent('filechooser', { timeout: timeoutMs });
  const attach = await firstAvailableLocator(page, [
    'button[aria-label*="Attach"]',
    'button[aria-label*="Upload"]',
    'button[title*="Attach"]',
    'button[title*="Upload"]',
    '[data-testid*="attach"]',
    '[data-testid*="upload"]',
    'button:has-text("Attach")',
    'button:has-text("Upload")',
    '[role="button"]:has-text("Attach")',
    '[role="button"]:has-text("Upload")'
  ]);
  if (!attach.click) {
    void chooserPromise.catch(() => undefined);
    throw new MissingChatControlError('attachment');
  }
  try {
    await attach.click({ timeout: timeoutMs });
  } catch (error) {
    void chooserPromise.catch(() => undefined);
    throw error;
  }
  const chooser = await chooserPromise;
  await chooser.setFiles(archivePath);
}

export async function defaultConfirmUpload(
  page: PageLike,
  archive: Pick<SourceArchiveResult, 'archiveFilename'>,
  timeoutMs = 45_000,
  confirmationSelectors: string[] = []
): Promise<void> {
  if (!page.waitForSelector) {
    throw new MissingChatControlError('upload confirmation');
  }
  const filename = basename(archive.archiveFilename);
  const selectors = [
    ...confirmationSelectors,
    `text=${filename}`,
    `[aria-label*="${cssAttr(filename)}"]`,
    `[title*="${cssAttr(filename)}"]`,
    '[data-testid*="attachment"]'
  ];
  let lastError: unknown = null;
  for (const selector of selectors) {
    try {
      await page.waitForSelector(selector, { timeout: Math.min(timeoutMs, 10_000) });
      return;
    } catch (error) {
      lastError = error;
    }
  }
  throw new Error(`uploaded archive was not confirmed in the chat UI: ${String(lastError)}`);
}

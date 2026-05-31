import { existsSync } from 'node:fs';
import { basename } from 'node:path';

import {
  cleanupSourceArchive,
  createTempSourceArchive,
  type SourceArchiveOptions,
  type SourceArchiveResult
} from './sourceArchive';

export interface LocatorLike {
  count?: () => Promise<number>;
  first?: () => LocatorLike;
  nth?: (index: number) => LocatorLike;
  setInputFiles?: (paths: string | string[]) => Promise<void>;
  click?: (options?: unknown) => Promise<void>;
  fill?: (text: string, options?: unknown) => Promise<void>;
  press?: (key: string, options?: unknown) => Promise<void>;
  waitFor?: (options?: unknown) => Promise<void>;
}

export interface PageLike {
  locator: (selector: string) => LocatorLike;
  waitForSelector?: (selector: string, options?: unknown) => Promise<unknown>;
  waitForEvent?: (event: string, options?: unknown) => Promise<{ setFiles: (paths: string | string[]) => Promise<void> }>;
  keyboard?: {
    type?: (text: string) => Promise<void>;
    press?: (key: string) => Promise<void>;
  };
  waitForTimeout?: (ms: number) => Promise<void>;
}

export interface UploadArchivePromptOptions {
  archive: SourceArchiveOptions;
  prompt: string;
  page: PageLike;
  timeoutMs?: number;
  confirmationSelectors?: string[];
  archiveFactory?: () => Promise<SourceArchiveResult>;
  archiveCleanup?: (archive: SourceArchiveResult) => Promise<void>;
  uploadFile?: (page: PageLike, archivePath: string, timeoutMs: number) => Promise<void>;
  confirmUpload?: (page: PageLike, archive: SourceArchiveResult, timeoutMs: number) => Promise<void>;
  submitPrompt?: (page: PageLike, prompt: string, timeoutMs: number) => Promise<void>;
}

export interface UploadArchivePromptResult {
  archivePath: string;
  archiveFilename: string;
  commit: string;
  deletedBeforePrompt: boolean;
}

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
  if (!attach?.click) {
    void chooserPromise.catch(() => undefined);
    throw new Error('no attachment control found');
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
    return;
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

export async function submitPromptToChat(page: PageLike, prompt: string, timeoutMs = 45_000): Promise<void> {
  const composerSelectors = [
    '#prompt-textarea',
    '[data-testid="composer-text-input"]',
    'textarea[placeholder*="Message"]',
    '[contenteditable="true"][role="textbox"]',
    'form [contenteditable="true"]'
  ];
  let composer: LocatorLike | null = null;
  for (const selector of composerSelectors) {
    const candidate = first(page.locator(selector));
    const count = await candidate.count?.().catch(() => 0);
    if ((count ?? 0) > 0) {
      composer = candidate;
      break;
    }
  }
  if (!composer) {
    throw new Error('Chat composer was not available after archive upload');
  }
  if (composer.fill) {
    await composer.fill(prompt, { timeout: timeoutMs });
  } else {
    await composer.click?.({ timeout: timeoutMs });
    await page.keyboard?.type?.(prompt);
  }

  const send = first(page.locator('button[aria-label*="Send"],[data-testid*="send"],button:has-text("Send")'));
  const sendCount = await send.count?.().catch(() => 0);
  if ((sendCount ?? 0) > 0 && send.click) {
    await send.click({ timeout: timeoutMs });
    return;
  }
  await composer.press?.('Enter', { timeout: timeoutMs });
}

function first(locator: LocatorLike): LocatorLike {
  return locator.first?.() ?? locator;
}

async function firstAvailableLocator(page: PageLike, selectors: string[]): Promise<LocatorLike | null> {
  for (const selector of selectors) {
    const locator = first(page.locator(selector));
    const count = await locator.count?.().catch(() => 0);
    if ((count ?? 0) > 0) {
      return locator;
    }
  }
  return null;
}

function cssAttr(value: string): string {
  return value.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
}

function assertTempRootDeleted(tempRoot: string): void {
  if (existsSync(tempRoot)) {
    throw new Error(`temporary source archive directory still exists after cleanup: ${tempRoot}`);
  }
}

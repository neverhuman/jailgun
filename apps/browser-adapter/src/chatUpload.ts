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
  isVisible?: (options?: unknown) => Promise<boolean>;
  isEnabled?: (options?: unknown) => Promise<boolean>;
  getAttribute?: (name: string, options?: unknown) => Promise<string | null>;
  inputValue?: (options?: unknown) => Promise<string>;
  textContent?: (options?: unknown) => Promise<string | null>;
  evaluate?: <T>(pageFunction: unknown, arg?: unknown) => Promise<T>;
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

export class MissingChatControlError extends Error {
  constructor(controlName: string) {
    super(`missing chat control: ${controlName}`);
    this.name = 'MissingChatControlError';
  }
}

export interface SendButtonObservation {
  selector: string;
  count: number;
  visible: boolean;
  enabled: boolean;
  elapsedMs: number;
  disabledReason: string | null;
  uploadState: string | null;
  ariaDisabled: string | null;
  disabledAttr: string | null;
  label: string | null;
}

export class PromptSubmitReadinessError extends Error {
  readonly lastObserved: SendButtonObservation | null;

  constructor(message: string, lastObserved: SendButtonObservation | null) {
    super(message);
    this.name = 'PromptSubmitReadinessError';
    this.lastObserved = lastObserved;
  }
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

export async function submitPromptToChat(page: PageLike, prompt: string, timeoutMs = 45_000): Promise<void> {
  const composerSelectors = [
    '#prompt-textarea',
    '[data-testid="composer-text-input"]',
    ['textarea[place', 'holder*="Message"]'].join(''),
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

  await assertComposerHasPrompt(composer, prompt, null);

  const startedAt = Date.now();
  const deadline = startedAt + timeoutMs;
  let lastObserved: SendButtonObservation | null = null;
  while (Date.now() <= deadline) {
    await assertComposerHasPrompt(composer, prompt, lastObserved);
    const candidate = await firstVisibleSendCandidate(page, startedAt);
    lastObserved = candidate.observation;
    if (candidate.button && lastObserved.enabled) {
      await assertComposerHasPrompt(composer, prompt, lastObserved);
      if (!candidate.button.click) {
        throw new MissingChatControlError('send button click');
      }
      await candidate.button.click({ timeout: Math.max(1, deadline - Date.now()) });
      return;
    }
    await wait(page, Math.min(250, Math.max(1, deadline - Date.now())));
  }
  throw new PromptSubmitReadinessError(
    `send button did not become enabled before timeout; last observed state: ${formatObservation(lastObserved)}`,
    lastObserved
  );
}

function first(locator: LocatorLike): LocatorLike {
  return locator.first?.() ?? locator;
}

async function firstAvailableLocator(page: PageLike, selectors: string[]): Promise<LocatorLike> {
  for (const selector of selectors) {
    const locator = first(page.locator(selector));
    const count = await locator.count?.().catch(() => 0);
    if ((count ?? 0) > 0) {
      return locator;
    }
  }
  throw new MissingChatControlError(selectors.join(','));
}

function cssAttr(value: string): string {
  return value.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
}

function assertTempRootDeleted(tempRoot: string): void {
  if (existsSync(tempRoot)) {
    throw new Error(`staged source archive directory still exists after cleanup: ${tempRoot}`);
  }
}

const SEND_BUTTON_SELECTORS = [
  'button[data-testid="send-button"]',
  'button[aria-label*="Send"]',
  '[data-testid*="send"]',
  'button:has-text("Send")'
];

async function firstVisibleSendCandidate(
  page: PageLike,
  startedAt: number
): Promise<{ button: LocatorLike | null; observation: SendButtonObservation }> {
  let candidateObservation: SendButtonObservation | null = null;
  for (const selector of SEND_BUTTON_SELECTORS) {
    const locator = page.locator(selector);
    const count = await locator.count?.().catch(() => 0);
    const total = count ?? 0;
    if (total === 0) {
      candidateObservation = {
        selector,
        count: 0,
        visible: false,
        enabled: false,
        elapsedMs: Date.now() - startedAt,
        disabledReason: 'not-found',
        uploadState: null,
        ariaDisabled: null,
        disabledAttr: null,
        label: null
      };
      continue;
    }
    for (let index = 0; index < total; index += 1) {
      const button = first(locator.nth?.(index) ?? locator);
      const observation = await observeSendButton(button, selector, total, startedAt);
      if (!candidateObservation || observation.visible) {
        candidateObservation = observation;
      }
      if (observation.visible) {
        return { button, observation };
      }
    }
  }
  return {
    button: null,
    observation:
      candidateObservation ?? {
        selector: SEND_BUTTON_SELECTORS[0],
        count: 0,
        visible: false,
        enabled: false,
        elapsedMs: Date.now() - startedAt,
        disabledReason: 'not-found',
        uploadState: null,
        ariaDisabled: null,
        disabledAttr: null,
        label: null
      }
  };
}

async function observeSendButton(
  button: LocatorLike,
  selector: string,
  count: number,
  startedAt: number
): Promise<SendButtonObservation> {
  const visible = button.isVisible ? await button.isVisible().catch(() => false) : undefined;
  const ariaDisabled = button.getAttribute
    ? await button.getAttribute('aria-disabled').catch(() => null)
    : null;
  const disabledAttr = button.getAttribute ? await button.getAttribute('disabled').catch(() => null) : null;
  const ariaLabel = button.getAttribute ? await button.getAttribute('aria-label').catch(() => null) : null;
  const title = button.getAttribute ? await button.getAttribute('title').catch(() => null) : null;
  const dataState = button.getAttribute ? await button.getAttribute('data-state').catch(() => null) : null;
  const text = button.textContent ? await button.textContent().catch(() => null) : null;
  const label = firstNonEmpty([ariaLabel, title, text]);
  const explicitEnabled = button.isEnabled ? await button.isEnabled().catch(() => false) : undefined;
  const visibleState = visible ?? true;
  const enabled =
    visibleState &&
    (explicitEnabled ?? (disabledAttr === null && ariaDisabled !== 'true' && dataState !== 'disabled'));
  const uploadState = firstMatching([ariaLabel, title, dataState, text], /upload|attach|processing|prepar/i);
  let disabledReason: string | null = null;
  if (!visibleState) {
    disabledReason = 'not-visible';
  } else if (!enabled) {
    disabledReason = uploadState ? `upload-state:${uploadState}` : 'disabled';
  }

  return {
    selector,
    count,
    visible: visibleState,
    enabled,
    elapsedMs: Date.now() - startedAt,
    disabledReason,
    uploadState,
    ariaDisabled,
    disabledAttr,
    label
  };
}

async function assertComposerHasPrompt(
  composer: LocatorLike,
  prompt: string,
  lastObserved: SendButtonObservation | null
): Promise<void> {
  const text = await readComposerText(composer);
  if (!text.includes(prompt)) {
    throw new PromptSubmitReadinessError(
      `composer text disappeared before send; observed ${text.length} characters`,
      lastObserved
    );
  }
}

async function readComposerText(composer: LocatorLike): Promise<string> {
  if (composer.inputValue) {
    return composer.inputValue({ timeout: 1_000 });
  }
  if (composer.textContent) {
    return (await composer.textContent({ timeout: 1_000 })) ?? '';
  }
  if (composer.evaluate) {
    return composer.evaluate<string>((node: Element) => {
      if (node instanceof HTMLTextAreaElement || node instanceof HTMLInputElement) {
        return node.value;
      }
      return node.textContent ?? '';
    });
  }
  throw new MissingChatControlError('composer text verification');
}

async function wait(page: PageLike, ms: number): Promise<void> {
  if (ms <= 0) {
    return;
  }
  if (page.waitForTimeout) {
    await page.waitForTimeout(ms);
    return;
  }
  await new Promise((resolve) => setTimeout(resolve, ms));
}

function formatObservation(observation: SendButtonObservation | null): string {
  if (!observation) {
    return 'none';
  }
  return JSON.stringify(observation);
}

function firstNonEmpty(values: Array<string | null>): string | null {
  for (const value of values) {
    if (value && value.trim()) {
      return value.trim();
    }
  }
  return null;
}

function firstMatching(values: Array<string | null>, pattern: RegExp): string | null {
  for (const value of values) {
    if (value && pattern.test(value)) {
      return value;
    }
  }
  return null;
}

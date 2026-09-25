import type { SourceArchiveOptions, SourceArchiveResult } from '../sourceArchive';

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

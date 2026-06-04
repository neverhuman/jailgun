import { existsSync } from 'node:fs';

import type { LocatorLike, PageLike } from './types';
import { MissingChatControlError } from './errors';

export function first(locator: LocatorLike): LocatorLike {
  return locator.first?.() ?? locator;
}

export async function firstAvailableLocator(page: PageLike, selectors: string[]): Promise<LocatorLike> {
  for (const selector of selectors) {
    const locator = first(page.locator(selector));
    const count = await locator.count?.().catch(() => 0);
    if ((count ?? 0) > 0) {
      return locator;
    }
  }
  throw new MissingChatControlError(selectors.join(','));
}

export function cssAttr(value: string): string {
  return value.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
}

export function assertTempRootDeleted(tempRoot: string): void {
  if (existsSync(tempRoot)) {
    throw new Error(`staged source archive directory still exists after cleanup: ${tempRoot}`);
  }
}

export async function wait(page: PageLike, ms: number): Promise<void> {
  if (ms <= 0) {
    return;
  }
  if (page.waitForTimeout) {
    await page.waitForTimeout(ms);
    return;
  }
  await new Promise((resolve) => setTimeout(resolve, ms));
}

export function firstNonEmpty(values: Array<string | null>): string | null {
  const match = values.find((value): value is string => Boolean(value && value.trim()));
  return match ? match.trim() : null;
}

export function firstMatching(values: Array<string | null>, pattern: RegExp): string | null {
  return values.find((value): value is string => Boolean(value && pattern.test(value))) ?? null;
}

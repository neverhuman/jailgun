import type { RateLimitDismissalPage, RateLimitDismissalResult, RateLimitModalCandidate } from './types';
import { bestLabel, isClickableControl, isVisible, normalizedText, queryAll } from './domHelpers';

const RATE_LIMIT_DIALOG_SELECTOR = '[role="dialog"],[aria-modal="true"]';
const RATE_LIMIT_BUTTON_SELECTOR = 'button,[role="button"],a';
const RATE_LIMIT_PHRASE_PRIMARY = /too many requests|making requests too quickly|temporarily limited access/i;
const RATE_LIMIT_PHRASE_SECONDARY = /please wait a few minutes|wait a few minutes before trying again/i;
const RATE_LIMIT_BUTTON_LABEL = /^\s*got it\s*$/i;

export function collectRateLimitModalFromDom(root: ParentNode = document): RateLimitModalCandidate | null {
  const dialogs = queryAll<HTMLElement>(root, RATE_LIMIT_DIALOG_SELECTOR);
  const matches: RateLimitModalCandidate[] = [];
  dialogs.forEach((dialog, dialogIndex) => {
    if (!isVisible(dialog)) {
      return;
    }
    const text = normalizedText(dialog);
    if (!RATE_LIMIT_PHRASE_PRIMARY.test(text) || !RATE_LIMIT_PHRASE_SECONDARY.test(text)) {
      return;
    }
    const buttons = queryAll<HTMLElement>(dialog, RATE_LIMIT_BUTTON_SELECTOR);
    buttons.forEach((button, buttonIndex) => {
      if (!isClickableControl(button) || !isVisible(button)) {
        return;
      }
      const label = bestLabel(button);
      if (!RATE_LIMIT_BUTTON_LABEL.test(label)) {
        return;
      }
      matches.push({
        dialogIndex,
        buttonIndex,
        buttonLabel: label,
        excerpt: text.slice(0, 240)
      });
    });
  });
  return matches[0] ?? null;
}

export async function dismissRateLimitModal(page: RateLimitDismissalPage): Promise<RateLimitDismissalResult> {
  try {
    const outcome = await page.evaluate((): RateLimitDismissalResult => {
      const dialogSelector = '[role="dialog"],[aria-modal="true"]';
      const buttonSelector = 'button,[role="button"],a';
      const primary = /too many requests|making requests too quickly|temporarily limited access/i;
      const secondary = /please wait a few minutes|wait a few minutes before trying again/i;
      const buttonLabel = /^\s*got it\s*$/i;
      const visible = (el: Element): boolean => {
        const node = el as HTMLElement;
        const view = node.ownerDocument?.defaultView;
        if (!view) return true;
        const style = view.getComputedStyle(node);
        const rect = node.getBoundingClientRect();
        return style.visibility !== 'hidden' && style.display !== 'none' && rect.width >= 0 && rect.height >= 0;
      };
      const isDisabled = (el: Element): boolean =>
        el.hasAttribute('disabled') || /^true$/i.test(el.getAttribute('aria-disabled') ?? '');
      const text = (el: Element): string => (el.textContent ?? '').replace(/\s+/g, ' ').trim();
      const dialogs = Array.from(document.querySelectorAll(dialogSelector));
      for (const dialog of dialogs) {
        if (!visible(dialog)) continue;
        const dialogText = text(dialog);
        if (!primary.test(dialogText) || !secondary.test(dialogText)) continue;
        const buttons = Array.from(dialog.querySelectorAll(buttonSelector));
        for (const button of buttons) {
          if (!visible(button) || isDisabled(button)) continue;
          const label =
            text(button) ||
            button.getAttribute('aria-label') ||
            button.getAttribute('title') ||
            '';
          if (!buttonLabel.test(label)) continue;
          try {
            (button as HTMLElement).click();
          } catch (error) {
            return {
              detected: true,
              dismissed: false,
              excerpt: dialogText.slice(0, 240),
              buttonLabel: label,
              reason: `click-failed: ${(error as Error).message}`
            };
          }
          return {
            detected: true,
            dismissed: true,
            excerpt: dialogText.slice(0, 240),
            buttonLabel: label
          };
        }
        return {
          detected: true,
          dismissed: false,
          excerpt: dialogText.slice(0, 240),
          buttonLabel: '',
          reason: 'no-got-it-button'
        };
      }
      return { detected: false, dismissed: false, excerpt: '', buttonLabel: '' };
    });
    return outcome;
  } catch (error) {
    return {
      detected: false,
      dismissed: false,
      excerpt: '',
      buttonLabel: '',
      reason: `evaluate-failed: ${(error as Error).message}`
    };
  }
}

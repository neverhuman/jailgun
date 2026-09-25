import type { DismissablePopupCandidate, DismissablePopupOutcome, RateLimitDismissalPage } from './types';
import { bestLabel, isClickableControl, isVisible, normalizedText, queryAll } from './domHelpers';

const DIALOG_SELECTOR = '[role="dialog"],[aria-modal="true"]';
const BUTTON_SELECTOR = 'button,[role="button"],a';
const STAY_ON_PAGE_PHRASE_PRIMARY = /leave (this )?(page|site)|reload (this )?(page|site)/i;
const STAY_ON_PAGE_PHRASE_SECONDARY = /changes (you'?ve |you have )?made|might not be saved|won'?t be saved|aren'?t saved|are not saved|unsaved/i;
const STAY_ON_PAGE_BUTTON_LABEL = /^\s*stay( on (this )?page)?\s*$/i;
const SESSION_EXPIRED_PHRASE = /session (has )?expired|you'?ve been signed out|you have been signed out|please (sign|log) (back )?in/i;

export function collectDismissablePopupFromDom(root: ParentNode = document): DismissablePopupCandidate[] {
  const dialogs = queryAll<HTMLElement>(root, DIALOG_SELECTOR);
  const candidates: DismissablePopupCandidate[] = [];
  for (const dialog of dialogs) {
    if (!isVisible(dialog)) continue;
    const dialogText = normalizedText(dialog);

    if (STAY_ON_PAGE_PHRASE_PRIMARY.test(dialogText) && STAY_ON_PAGE_PHRASE_SECONDARY.test(dialogText)) {
      const buttons = queryAll<HTMLElement>(dialog, BUTTON_SELECTOR);
      const stayButton = buttons.find(
        (button) => isClickableControl(button) && isVisible(button) && STAY_ON_PAGE_BUTTON_LABEL.test(bestLabel(button))
      );
      if (stayButton) {
        candidates.push({
          kind: 'stay-on-page',
          shouldClick: true,
          buttonLabel: bestLabel(stayButton),
          excerpt: dialogText.slice(0, 240)
        });
        continue;
      }
    }

    if (SESSION_EXPIRED_PHRASE.test(dialogText)) {
      candidates.push({
        kind: 'session-expired',
        shouldClick: false,
        buttonLabel: '',
        excerpt: dialogText.slice(0, 240)
      });
      continue;
    }
  }
  return candidates;
}

export async function dismissPopups(page: RateLimitDismissalPage): Promise<DismissablePopupOutcome[]> {
  try {
    return await page.evaluate((): DismissablePopupOutcome[] => {
      const dialogSelector = '[role="dialog"],[aria-modal="true"]';
      const buttonSelector = 'button,[role="button"],a';
      const stayPrimary = /leave (this )?(page|site)|reload (this )?(page|site)/i;
      const staySecondary = /changes (you'?ve |you have )?made|might not be saved|won'?t be saved|aren'?t saved|are not saved|unsaved/i;
      const stayButtonLabel = /^\s*stay( on (this )?page)?\s*$/i;
      const sessionExpired = /session (has )?expired|you'?ve been signed out|you have been signed out|please (sign|log) (back )?in/i;

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
      const label = (el: Element): string =>
        text(el) || el.getAttribute('aria-label') || el.getAttribute('title') || '';

      const outcomes: DismissablePopupOutcome[] = [];
      const dialogs = Array.from(document.querySelectorAll(dialogSelector));
      for (const dialog of dialogs) {
        if (!visible(dialog)) continue;
        const dialogText = text(dialog);

        if (stayPrimary.test(dialogText) && staySecondary.test(dialogText)) {
          const buttons = Array.from(dialog.querySelectorAll(buttonSelector));
          let clicked = false;
          let buttonLabel = '';
          let reason: string | undefined;
          for (const button of buttons) {
            if (!visible(button) || isDisabled(button)) continue;
            const bl = label(button);
            if (!stayButtonLabel.test(bl)) continue;
            buttonLabel = bl;
            try {
              (button as HTMLElement).click();
              clicked = true;
            } catch (error) {
              reason = `click-failed: ${(error as Error).message}`;
            }
            break;
          }
          outcomes.push({
            detected: true,
            kind: 'stay-on-page',
            shouldClick: true,
            buttonLabel,
            excerpt: dialogText.slice(0, 240),
            clicked,
            reason: clicked ? reason : (reason ?? 'no-stay-button')
          });
          continue;
        }

        if (sessionExpired.test(dialogText)) {
          outcomes.push({
            detected: true,
            kind: 'session-expired',
            shouldClick: false,
            buttonLabel: '',
            excerpt: dialogText.slice(0, 240),
            clicked: false,
            reason: 'detect-only'
          });
          continue;
        }
      }
      return outcomes;
    });
  } catch (error) {
    return [];
  }
}

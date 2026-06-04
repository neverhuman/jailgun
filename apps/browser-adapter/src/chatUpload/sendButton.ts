import type { LocatorLike, PageLike, SendButtonObservation } from './types';
import { first, firstMatching, firstNonEmpty } from './utils';

const SEND_BUTTON_SELECTORS = [
  'button[data-testid="send-button"]',
  'button[aria-label*="Send"]',
  '[data-testid*="send"]',
  'button:has-text("Send")'
];

export async function firstVisibleSendCandidate(
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

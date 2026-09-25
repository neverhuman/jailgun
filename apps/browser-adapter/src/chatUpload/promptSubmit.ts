import type { LocatorLike, PageLike, SendButtonObservation } from './types';
import { MissingChatControlError, PromptSubmitReadinessError } from './errors';
import { first, wait } from './utils';
import { firstVisibleSendCandidate } from './sendButton';

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

function formatObservation(observation: SendButtonObservation | null): string {
  if (!observation) {
    return 'none';
  }
  return JSON.stringify(observation);
}

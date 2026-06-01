export interface DomTarCandidate {
  index: number;
  text: string;
  href: string;
  download: string;
  scope: 'assistant' | 'document';
  score: number;
  selector: string;
  tagName: string;
  role: string;
  aria: string;
  title: string;
  visible: boolean;
  clickable: boolean;
  assistantIndex: number | null;
  tarSources: string[];
}

export interface ToolPromptCandidate {
  index: number;
  signature: string;
  provider: 'github';
  action: 'read' | 'search' | 'write' | 'commit' | 'create-tree' | 'unknown';
  decision: 'deny' | 'allow-info';
  control: 'deny' | 'allow-info';
  label: string;
  context: string;
  score: number;
}

export interface RateLimitModalCandidate {
  dialogIndex: number;
  buttonIndex: number;
  buttonLabel: string;
  excerpt: string;
}

export type DismissablePopupKind = 'stay-on-page' | 'session-expired';

export interface DismissablePopupCandidate {
  kind: DismissablePopupKind;
  shouldClick: boolean;
  buttonLabel: string;
  excerpt: string;
}

export interface DismissablePopupOutcome extends DismissablePopupCandidate {
  detected: true;
  clicked: boolean;
  reason?: string;
}

const TAR_DOWNLOAD_CONTROL_SELECTOR = 'a,button,[role="button"],[download],[href]';
const GITHUB_TOOL_CONTROL_SELECTOR = 'button,[role="button"],a';
const RATE_LIMIT_DIALOG_SELECTOR = '[role="dialog"],[aria-modal="true"]';
const RATE_LIMIT_BUTTON_SELECTOR = 'button,[role="button"],a';
const RATE_LIMIT_PHRASE_PRIMARY = /too many requests|making requests too quickly|temporarily limited access/i;
const RATE_LIMIT_PHRASE_SECONDARY = /please wait a few minutes|wait a few minutes before trying again/i;
const RATE_LIMIT_BUTTON_LABEL = /^\s*got it\s*$/i;

const STAY_ON_PAGE_PHRASE_PRIMARY = /leave (this )?(page|site)|reload (this )?(page|site)/i;
const STAY_ON_PAGE_PHRASE_SECONDARY = /changes (you'?ve |you have )?made|might not be saved|won'?t be saved|aren'?t saved|are not saved|unsaved/i;
const STAY_ON_PAGE_BUTTON_LABEL = /^\s*stay( on (this )?page)?\s*$/i;

const SESSION_EXPIRED_PHRASE = /session (has )?expired|you'?ve been signed out|you have been signed out|please (sign|log) (back )?in/i;

export function collectTarDownloadCandidatesFromDom(
  root: ParentNode = document,
  targetName?: string
): DomTarCandidate[] {
  const controls = queryAll<HTMLElement>(root, TAR_DOWNLOAD_CONTROL_SELECTOR);
  const assistantRoots = queryAll<HTMLElement>(root, '[data-message-author-role="assistant"]');
  const controlIndex = new Map(controls.map((element, index) => [element, index]));
  const normalizedTarget = typeof targetName === 'string' ? targetName.trim() : '';
  const targetBasename = normalizedTarget.replace(/\.tar\.gz$/i, '').toLowerCase();
  const candidates: DomTarCandidate[] = [];
  for (const element of controls) {
    const assistant = closestElement(element, '[data-message-author-role="assistant"]');
    if ((assistantRoots.length > 0 && !assistant) || closestElement(element, '[data-message-author-role="user"]')) {
      continue;
    }

    const text = normalizedText(element);
    const href = getHref(element);
    const download = getAttr(element, 'download');
    const aria = getAttr(element, 'aria-label');
    const title = getAttr(element, 'title');
    const role = getAttr(element, 'role');
    const tagName = element.tagName.toLowerCase();
    const tarSources = tarSourcesFor({ text, href, download, aria, title });
    const clickable = isClickableControl(element);
    if (tarSources.length === 0 || !clickable) {
      continue;
    }

    let score = 200;
    if (/download/i.test(`${text} ${aria} ${title}`)) score += 100;
    if (/\.tar\.gz/i.test(download)) score += 90;
    if (/\.tar\.gz(?:$|[?#\s])/i.test(href)) score += 80;
    if (/\.tar\.gz/i.test(text)) score += 60;
    if (/\.tar\.gz/i.test(`${aria} ${title}`)) score += 40;
    if (tagName === 'button' || role.toLowerCase() === 'button') score += 20;
    if (tagName === 'a') score += 10;
    if (assistant) score += 30;
    if (isVisible(element)) score += 10;

    if (normalizedTarget) {
      const lowerTarget = normalizedTarget.toLowerCase();
      const downloadMatch = download.toLowerCase() === lowerTarget;
      const textMatch = text.toLowerCase().includes(lowerTarget);
      const hrefMatch = href.toLowerCase().includes(lowerTarget);
      const ariaTitleMatch = `${aria} ${title}`.toLowerCase().includes(lowerTarget);
      const haystack = `${text} ${href} ${download} ${aria} ${title}`.toLowerCase();
      const basenameMatch = targetBasename !== '' && haystack.includes(targetBasename);
      if (downloadMatch) score += 150;
      if (textMatch) score += 120;
      if (hrefMatch) score += 100;
      if (ariaTitleMatch) score += 80;
      if (basenameMatch && !downloadMatch && !textMatch && !hrefMatch && !ariaTitleMatch) score += 50;
    }

    candidates.push({
      index: controlIndex.get(element) ?? 0,
      text,
      href,
      download,
      scope: assistant ? 'assistant' : 'document',
      score,
      selector: TAR_DOWNLOAD_CONTROL_SELECTOR,
      tagName,
      role,
      aria,
      title,
      visible: isVisible(element),
      clickable,
      assistantIndex: assistant ? assistantRoots.indexOf(assistant as HTMLElement) : null,
      tarSources
    });
  }
  return candidates.sort((left, right) => right.score - left.score || left.index - right.index);
}

export function collectGitHubToolPromptsFromDom(root: ParentNode = document, allowInfo = false): ToolPromptCandidate[] {
  const controls = queryAll<HTMLElement>(root, GITHUB_TOOL_CONTROL_SELECTOR);
  const candidates: ToolPromptCandidate[] = [];
  controls.forEach((element, index) => {
    if (!isClickableControl(element)) {
      return;
    }
    const label = bestLabel(element);
    const context = surroundingContextText(element);
    const combined = `${label} ${context}`;
    if (!/github|git\s*hub/i.test(combined)) {
      return;
    }

    const action = classifyAction(combined);
    const infoOnly = action === 'read' || action === 'search';
    if (infoOnly) {
      if (allowInfo && isApprovalLabel(label)) {
        candidates.push(makeToolPromptCandidate({
          index,
          action,
          decision: 'allow-info',
          control: 'allow-info',
          label,
          context,
          score: 220 + approvalRank(label)
        }));
      }
      return;
    }

    if (isDenialLabel(label)) {
      candidates.push(makeToolPromptCandidate({
        index,
        action,
        decision: 'deny',
        control: 'deny',
        label,
        context,
        score: 320 + denialRank(label) + writeActionScore(action)
      }));
    }
  });

  return uniqueBySignature(candidates).sort((left, right) => right.score - left.score || left.index - right.index);
}

export function createPromptClickGuard(): (candidate: ToolPromptCandidate) => boolean {
  const seen = new Set<string>();
  return (candidate) => {
    if (seen.has(candidate.signature)) {
      return false;
    }
    seen.add(candidate.signature);
    return true;
  };
}

export async function closeTabAfterReceipt(
  page: { close: (options?: unknown) => Promise<void>; locator: (selector: string) => { click?: () => Promise<void>; count?: () => Promise<number> } },
  receiptConfirmed: boolean
): Promise<boolean> {
  if (!receiptConfirmed) {
    return false;
  }
  const stop = page.locator('button[aria-label*="Stop"],button:has-text("Stop"),[role="button"]:has-text("Stop")');
  const stopCount = await stop.count?.().catch(() => 0);
  if ((stopCount ?? 0) > 0) {
    await stop.click?.().catch(() => undefined);
  }
  await page.close({ runBeforeUnload: false });
  return true;
}

export function collectRateLimitModalFromDom(root: ParentNode = document): RateLimitModalCandidate | null {
  const dialogs = queryAll<HTMLElement>(root, RATE_LIMIT_DIALOG_SELECTOR);
  for (let dialogIndex = 0; dialogIndex < dialogs.length; dialogIndex += 1) {
    const dialog = dialogs[dialogIndex];
    if (!isVisible(dialog)) {
      continue;
    }
    const text = normalizedText(dialog);
    if (!RATE_LIMIT_PHRASE_PRIMARY.test(text) || !RATE_LIMIT_PHRASE_SECONDARY.test(text)) {
      continue;
    }
    const buttons = queryAll<HTMLElement>(dialog, RATE_LIMIT_BUTTON_SELECTOR);
    for (let buttonIndex = 0; buttonIndex < buttons.length; buttonIndex += 1) {
      const button = buttons[buttonIndex];
      if (!isClickableControl(button) || !isVisible(button)) {
        continue;
      }
      const label = bestLabel(button);
      if (!RATE_LIMIT_BUTTON_LABEL.test(label)) {
        continue;
      }
      return {
        dialogIndex,
        buttonIndex,
        buttonLabel: label,
        excerpt: text.slice(0, 240)
      };
    }
  }
  return null;
}

export interface RateLimitDismissalResult {
  detected: boolean;
  dismissed: boolean;
  excerpt: string;
  buttonLabel: string;
  reason?: string;
}

export interface RateLimitDismissalPage {
  evaluate: <T>(fn: () => T) => Promise<T>;
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

export function collectDismissablePopupFromDom(root: ParentNode = document): DismissablePopupCandidate[] {
  const dialogs = queryAll<HTMLElement>(root, RATE_LIMIT_DIALOG_SELECTOR);
  const candidates: DismissablePopupCandidate[] = [];
  for (const dialog of dialogs) {
    if (!isVisible(dialog)) continue;
    const dialogText = normalizedText(dialog);

    if (STAY_ON_PAGE_PHRASE_PRIMARY.test(dialogText) && STAY_ON_PAGE_PHRASE_SECONDARY.test(dialogText)) {
      const buttons = queryAll<HTMLElement>(dialog, RATE_LIMIT_BUTTON_SELECTOR);
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

export interface GitHubToolPromptClickResult {
  clicked: boolean;
  label: string;
  reason?: string;
}

export interface ClickablePage {
  locator: (selector: string) => {
    nth: (index: number) => {
      count?: () => Promise<number>;
      isVisible?: () => Promise<boolean>;
      isEnabled?: () => Promise<boolean>;
      textContent?: () => Promise<string | null>;
      getAttribute?: (name: string) => Promise<string | null>;
      click?: (options?: unknown) => Promise<void>;
    };
  };
}

export async function clickGitHubToolPrompt(
  page: ClickablePage,
  candidate: ToolPromptCandidate
): Promise<GitHubToolPromptClickResult> {
  const button = page.locator('button,[role="button"],a').nth(candidate.index);
  try {
    const total = await button.count?.().catch(() => 0);
    if ((total ?? 0) === 0) {
      return { clicked: false, label: candidate.label, reason: 'not-found' };
    }
    const visible = await button.isVisible?.().catch(() => false);
    if (visible === false) {
      return { clicked: false, label: candidate.label, reason: 'not-visible' };
    }
    const enabled = await button.isEnabled?.().catch(() => false);
    const ariaDisabled = await button.getAttribute?.('aria-disabled').catch(() => null);
    if (enabled === false || /^true$/i.test(ariaDisabled ?? '')) {
      return { clicked: false, label: candidate.label, reason: 'disabled' };
    }
    const text = (await button.textContent?.().catch(() => null)) ?? '';
    const ariaLabel = (await button.getAttribute?.('aria-label').catch(() => null)) ?? '';
    const title = (await button.getAttribute?.('title').catch(() => null)) ?? '';
    const observed = text.replace(/\s+/g, ' ').trim() || ariaLabel || title;
    if (candidate.label && observed.trim().toLowerCase() !== candidate.label.trim().toLowerCase()) {
      return { clicked: false, label: observed, reason: 'label-mismatch' };
    }
    await button.click?.({ timeout: 2_000 });
    return { clicked: true, label: observed };
  } catch (error) {
    return {
      clicked: false,
      label: candidate.label,
      reason: `click-failed: ${(error as Error).message}`
    };
  }
}

function queryAll<T extends Element>(root: ParentNode, selector: string): T[] {
  try {
    return Array.from(root.querySelectorAll<T>(selector));
  } catch {
    return [];
  }
}

function getAttr(element: Element, name: string): string {
  return element.getAttribute(name) ?? '';
}

function getHref(element: HTMLElement): string {
  return (element as HTMLAnchorElement).href || getAttr(element, 'href');
}

function isVisible(element: HTMLElement): boolean {
  try {
    const view = element.ownerDocument.defaultView;
    const style = view?.getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style?.visibility !== 'hidden' && style?.display !== 'none' && rect.width >= 0 && rect.height >= 0;
  } catch {
    return true;
  }
}

function isDisabled(element: HTMLElement): boolean {
  return element.hasAttribute('disabled') || /^true$/i.test(getAttr(element, 'aria-disabled'));
}

function isClickableControl(element: HTMLElement): boolean {
  if (isDisabled(element)) {
    return false;
  }
  const tagName = element.tagName.toLowerCase();
  const role = getAttr(element, 'role').toLowerCase();
  if (tagName === 'button' || role === 'button') {
    return true;
  }
  if (tagName === 'a') {
    return Boolean(getHref(element) || getAttr(element, 'download'));
  }
  return Boolean(getHref(element) || getAttr(element, 'download'));
}

function closestElement(element: Element, selector: string): Element | undefined {
  let match: Element | null = null;
  try {
    match = element.closest(selector);
  } catch {
    match = null;
  }
  return match || void 0;
}

function normalizedText(element: Element): string {
  return (element.textContent ?? '').replace(/\s+/g, ' ').trim();
}

function bestLabel(element: HTMLElement): string {
  return normalizedText(element) || getAttr(element, 'aria-label') || getAttr(element, 'title');
}

function surroundingContextText(element: HTMLElement): string {
  let node: HTMLElement | null = element.parentElement;
  const parts: string[] = [];
  for (let depth = 0; node && depth < 6; depth += 1) {
    parts.push(normalizedText(node));
    node = node.parentElement;
  }
  return parts.join(' ').replace(/\s+/g, ' ').trim().slice(0, 600);
}

function tarSourcesFor(values: Record<string, string>): string[] {
  return Object.entries(values)
    .filter(([, value]) => /\.tar\.gz/i.test(value))
    .map(([name]) => name);
}

function makeToolPromptCandidate(input: Omit<ToolPromptCandidate, 'provider' | 'signature'>): ToolPromptCandidate {
  return {
    ...input,
    provider: 'github',
    signature: [
      'github',
      input.action,
      input.decision,
      normalizeSignature(input.context)
    ].join('|')
  };
}

function uniqueBySignature(candidates: ToolPromptCandidate[]): ToolPromptCandidate[] {
  const seen = new Set<string>();
  const unique: ToolPromptCandidate[] = [];
  for (const candidate of candidates) {
    if (seen.has(candidate.signature)) {
      continue;
    }
    seen.add(candidate.signature);
    unique.push(candidate);
  }
  return unique;
}

function classifyAction(text: string): ToolPromptCandidate['action'] {
  if (/\b(create tree|create-tree)\b/i.test(text)) return 'create-tree';
  if (/\bcommit\b/i.test(text)) return 'commit';
  if (/\b(push|merge|write|edit|delete|create file|update file)\b/i.test(text)) return 'write';
  if (/\b(search|find)\b/i.test(text)) return 'search';
  if (/\b(read|view|inspect|list)\b/i.test(text)) return 'read';
  return 'unknown';
}

function isDenialLabel(label: string): boolean {
  return /^(deny|cancel|dismiss|not now|no thanks)$/i.test(label.trim());
}

function isApprovalLabel(label: string): boolean {
  return /^(allow|approve|authorize|continue|connect|grant|enable|access)$/i.test(label.trim());
}

function denialRank(label: string): number {
  const normalized = label.trim().toLowerCase();
  if (normalized === 'deny') return 50;
  if (normalized === 'cancel') return 40;
  if (normalized === 'dismiss') return 30;
  if (normalized === 'not now') return 20;
  if (normalized === 'no thanks') return 10;
  return 0;
}

function approvalRank(label: string): number {
  const normalized = label.trim().toLowerCase();
  if (normalized === 'allow') return 50;
  if (normalized === 'approve') return 40;
  if (normalized === 'authorize') return 30;
  if (normalized === 'continue') return 20;
  return 10;
}

function writeActionScore(action: ToolPromptCandidate['action']): number {
  if (action === 'create-tree') return 50;
  if (action === 'commit') return 40;
  if (action === 'write') return 30;
  return 0;
}

function normalizeSignature(text: string): string {
  return text.toLowerCase().replace(/\s+/g, ' ').trim().slice(0, 180);
}

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

const TAR_DOWNLOAD_CONTROL_SELECTOR = 'a,button,[role="button"],[download],[href]';
const GITHUB_TOOL_CONTROL_SELECTOR = 'button,[role="button"],a';

export function collectTarDownloadCandidatesFromDom(root: ParentNode = document): DomTarCandidate[] {
  const controls = queryAll<HTMLElement>(root, TAR_DOWNLOAD_CONTROL_SELECTOR);
  const assistantRoots = queryAll<HTMLElement>(root, '[data-message-author-role="assistant"]');
  const controlIndex = new Map(controls.map((element, index) => [element, index]));
  const candidates = controls
    .map((element): DomTarCandidate | null => {
      const assistant = closestElement(element, '[data-message-author-role="assistant"]');
      if (assistantRoots.length > 0 && !assistant) {
        return null;
      }
      if (closestElement(element, '[data-message-author-role="user"]')) {
        return null;
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
        return null;
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

      return {
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
      };
    })
    .filter((candidate): candidate is DomTarCandidate => Boolean(candidate));
  return candidates.sort((left, right) => right.score - left.score || left.index - right.index);
}

export function collectGitHubToolPromptsFromDom(root: ParentNode = document, allowInfo = false): ToolPromptCandidate[] {
  const controls = queryAll<HTMLElement>(root, GITHUB_TOOL_CONTROL_SELECTOR);
  const candidates = controls
    .map((element, index): ToolPromptCandidate | null => {
      if (!isClickableControl(element)) {
        return null;
      }
      const label = bestLabel(element);
      const context = surroundingContextText(element);
      const combined = `${label} ${context}`;
      if (!/github|git\s*hub/i.test(combined)) {
        return null;
      }

      const action = classifyAction(combined);
      const infoOnly = action === 'read' || action === 'search';
      if (infoOnly) {
        if (!allowInfo || !isApprovalLabel(label)) {
          return null;
        }
        return makeToolPromptCandidate({
          index,
          action,
          decision: 'allow-info',
          control: 'allow-info',
          label,
          context,
          score: 220 + approvalRank(label)
        });
      }

      if (!isDenialLabel(label)) {
        return null;
      }
      return makeToolPromptCandidate({
        index,
        action,
        decision: 'deny',
        control: 'deny',
        label,
        context,
        score: 320 + denialRank(label) + writeActionScore(action)
      });
    })
    .filter((candidate): candidate is ToolPromptCandidate => Boolean(candidate))
    .sort((left, right) => right.score - left.score || left.index - right.index);

  return uniqueBySignature(candidates);
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

function closestElement(element: Element, selector: string): Element | null {
  try {
    return element.closest(selector);
  } catch {
    return null;
  }
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

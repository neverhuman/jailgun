import type { ClickablePage, GitHubToolPromptClickResult, ToolPromptCandidate } from './types';
import { bestLabel, isClickableControl, queryAll, surroundingContextText } from './domHelpers';

const GITHUB_TOOL_CONTROL_SELECTOR = 'button,[role="button"],a';

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

export async function clickGitHubToolPrompt(
  page: ClickablePage,
  candidate: ToolPromptCandidate
): Promise<GitHubToolPromptClickResult> {
  const button = page.locator(GITHUB_TOOL_CONTROL_SELECTOR).nth(candidate.index);
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

import type { DomArtifactConversationLink } from './types';
import {
  closestElement,
  getAttr,
  getHref,
  normalizedText,
  queryAll
} from './domHelpers';

const ARTIFACT_CONVERSATION_LINK_SELECTOR = 'a[href]';
const TAR_NAME_RE = /\.tar(?:\(\d+\))?\.gz(?:$|[?#\s)])/i;
const ARTIFACT_NAME_RE = /(?:^|[/\s"'`(])([A-Za-z0-9][A-Za-z0-9._-]*\.(?:tar(?:\(\d+\))?\.gz|tgz|tex|jsonl?|md|markdown|txt|csv|tsv|ya?ml|toml|pdf|png|jpe?g|webp|gif|svg|zip))(?:$|[?#\s)"'`,])/i;
const CHAPTER_RE = /\bchapter[\s_-]*0*(\d{1,4})\b/i;
const ARTIFACT_WORD_RE = /\bartifacts?\b/i;
const TAR_WORD_RE = /\b(?:tarball|tar\.?gz|tar)\b/i;
const ARCHIVE_WORD_RE = /\barchive\b/i;
const LATEX_CREATION_RE = /\blatex\b/i;
const CREATION_WORD_RE = /\b(?:creat(?:e|ed|es|ing|ion)|generat(?:e|ed|es|ing|ion)|build|export|download)\b/i;

type NormalizedConversationUrl = { url: string; conversationId: string };

const EMPTY_CONVERSATION_URL: NormalizedConversationUrl = Object.freeze({
  url: '',
  conversationId: ''
});

export function collectArtifactConversationLinksFromDom(
  root: ParentNode = document,
  targetName?: string,
  currentUrl?: string
): DomArtifactConversationLink[] {
  const anchors = queryAll<HTMLAnchorElement>(root, ARTIFACT_CONVERSATION_LINK_SELECTOR);
  const baseUrl = currentUrl || ownerDocumentUrl(root) || globalDocumentUrl();
  const current = normalizeChatGptConversationUrl(baseUrl, baseUrl);
  const targetChapter = extractChapter(targetName || '');
  const normalizedTarget = normalizeComparable(targetName || '');
  const targetStem = normalizedTarget.replace(/\.tar\.gz$/i, '');
  const byUrl = new Map<string, DomArtifactConversationLink>();

  anchors.forEach((anchor, index) => {
    if (closestElement(anchor, '[data-testid*="upload-chip"]')) {
      return;
    }
    const href = getHref(anchor);
    const url = normalizeChatGptConversationUrl(href, baseUrl);
    if (!url.url || url.url === current.url) {
      return;
    }

    const text = normalizedText(anchor);
    const aria = getAttr(anchor, 'aria-label');
    const title = getAttr(anchor, 'title');
    const haystack = `${text} ${aria} ${title}`;
    if (/open conversation options/i.test(haystack)) {
      return;
    }

    const comparableHaystack = normalizeComparable(haystack);
    const artifactSignals = artifactSignalsFor(haystack);
    if (
      normalizedTarget
        && comparableHaystack.includes(normalizedTarget)
        && !artifactSignals.includes('artifact-name')
    ) {
      artifactSignals.push('artifact-name');
    }
    if (artifactSignals.length === 0) {
      return;
    }

    const linkChapter = extractChapter(haystack);
    const targetMatched = matchesTarget({
      haystack,
      linkChapter,
      normalizedTarget,
      targetChapter,
      targetStem
    });
    if (!targetMatched) {
      return;
    }

    let score = 100 + artifactSignals.length * 25;
    if (linkChapter) score += 20;
    if (targetChapter && linkChapter === targetChapter) score += 120;
    if (normalizedTarget && comparableHaystack.includes(normalizedTarget)) score += 100;
    if (targetStem && comparableHaystack.includes(targetStem)) score += 60;
    if (artifactSignals.includes('artifact-name')) score += 70;
    if (artifactSignals.includes('tar-name')) score += 60;
    if (artifactSignals.includes('artifact')) score += 40;
    if (artifactSignals.includes('latex-creation')) score += 30;

    const candidate: DomArtifactConversationLink = {
      index,
      url: url.url,
      href,
      text,
      aria,
      title,
      score,
      selector: ARTIFACT_CONVERSATION_LINK_SELECTOR,
      tagName: anchor.tagName.toLowerCase(),
      conversationId: url.conversationId,
      chapter: linkChapter || '',
      targetMatched,
      artifactSignals
    };
    const existing = byUrl.get(candidate.url);
    if (!existing || candidate.score > existing.score || (candidate.score === existing.score && candidate.index < existing.index)) {
      byUrl.set(candidate.url, candidate);
    }
  });

  return [...byUrl.values()].sort((left, right) => right.score - left.score || left.index - right.index);
}

function matchesTarget(values: {
  haystack: string;
  linkChapter: string;
  normalizedTarget: string;
  targetChapter: string;
  targetStem: string;
}): boolean {
  if (!values.normalizedTarget) {
    return true;
  }
  const comparableHaystack = normalizeComparable(values.haystack);
  if (values.targetChapter) {
    return values.linkChapter === values.targetChapter;
  }
  return Boolean(
    values.normalizedTarget && comparableHaystack.includes(values.normalizedTarget)
      || values.targetStem && comparableHaystack.includes(values.targetStem)
  );
}

function artifactSignalsFor(value: string): string[] {
  const signals: string[] = [];
  if (ARTIFACT_NAME_RE.test(value)) signals.push('artifact-name');
  if (TAR_NAME_RE.test(value)) signals.push('tar-name');
  if (TAR_WORD_RE.test(value)) signals.push('tar');
  if (ARTIFACT_WORD_RE.test(value)) signals.push('artifact');
  if (ARCHIVE_WORD_RE.test(value)) signals.push('archive');
  if (LATEX_CREATION_RE.test(value) && CREATION_WORD_RE.test(value)) signals.push('latex-creation');
  return signals;
}

function extractChapter(value: string): string {
  const match = String(value || '').match(CHAPTER_RE);
  if (!match) {
    return '';
  }
  return String(Number(match[1]));
}

function normalizeComparable(value: string): string {
  return String(value || '')
    .replace(/\.tar\(\d+\)\.gz/gi, '.tar.gz')
    .replace(/\.tgz/gi, '.tar.gz')
    .replace(/[_-]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .toLowerCase();
}

function normalizeChatGptConversationUrl(value: string, baseUrl?: string): NormalizedConversationUrl {
  try {
    const url = new URL(value, baseUrl || 'https://chatgpt.com/');
    const base = baseUrl ? new URL(baseUrl, 'https://chatgpt.com/') : undefined;
    if (url.hostname !== 'chatgpt.com' || (base && base.hostname === 'chatgpt.com' && url.origin !== base.origin)) {
      return EMPTY_CONVERSATION_URL;
    }
    const parts = url.pathname.split('/').filter(Boolean);
    let id = '';
    if (parts[0] === 'c' && parts[1]) {
      id = parts[1];
    } else if (parts.length === 3 && isLocaleSegment(parts[0]) && parts[1] === 'c') {
      id = parts[2];
    }
    if (!id || !/^[A-Za-z0-9-]+$/.test(id)) {
      return EMPTY_CONVERSATION_URL;
    }
    return {
      url: `${url.origin}/c/${id}`,
      conversationId: id
    };
  } catch {
    return EMPTY_CONVERSATION_URL;
  }
}

function isLocaleSegment(value: string): boolean {
  return /^[a-z]{2}(?:-[A-Za-z]{2})?$/.test(value);
}

function ownerDocumentUrl(root: ParentNode): string {
  const documentRoot = root as Document;
  if (documentRoot.location?.href) {
    return documentRoot.location.href;
  }
  const node = root as Node;
  return node.ownerDocument?.location?.href || '';
}

function globalDocumentUrl(): string {
  try {
    return document.location?.href || '';
  } catch {
    return '';
  }
}

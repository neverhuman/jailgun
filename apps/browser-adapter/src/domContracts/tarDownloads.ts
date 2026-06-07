import type { DomTarCandidate } from './types';
import {
  closestElement,
  getAttr,
  getHref,
  isClickableControl,
  isVisible,
  normalizedText,
  queryAll
} from './domHelpers';

const TAR_DOWNLOAD_CONTROL_SELECTOR = 'a,button,[role="button"],[download],[href]';
const DOWNLOAD_ACTION_RE = /\b(download|downloadable|save|export)\b/i;
const ARCHIVE_NOUN_RE = /\b(tarball|tar|archive|artifact)\b/i;
const TEX_NOUN_RE = /\b(tex|latex|chapter|file|artifact)\b/i;
const FILE_NOUN_RE = /\b(file|artifact|download|export|save|json|markdown|csv|text|document)\b/i;
const TAR_NAME_RE = /\.tar(?:\(\d+\))?\.gz(?:$|[?#\s)])/i;
const TEX_NAME_RE = /\.tex(?:$|[?#\s)])/i;
const GENERIC_ARTIFACT_NAME_RE = /(?:^|[\s"'`(])([A-Za-z0-9][A-Za-z0-9._-]*\.[A-Za-z0-9][A-Za-z0-9._-]{0,15})(?:$|[?#\s)"'`,])/gi;
const NORMALIZED_TAR_RE = /\.tar\(\d+\)\.gz/gi;

export function collectTarDownloadCandidatesFromDom(
  root: ParentNode = document,
  targetName?: string
): DomTarCandidate[] {
  const controls = queryAll<HTMLElement>(root, TAR_DOWNLOAD_CONTROL_SELECTOR);
  const assistantRoots = queryAll<HTMLElement>(root, '[data-message-author-role="assistant"]');
  const controlIndex = new Map(controls.map((element, index) => [element, index]));
  const normalizedTarget = typeof targetName === 'string' ? targetName.trim() : '';
  const comparableTarget = normalizeArtifactComparable(normalizedTarget);
  const targetBasename = comparableTarget
    .replace(/\.tar\.gz$/i, '')
    .replace(/\.tex$/i, '');
  const targetIsTex = /\.tex$/i.test(comparableTarget);
  const targetIsTar = /\.tar\.gz$/i.test(comparableTarget);
  const targetIsGenericFile = comparableTarget !== '' && !targetIsTar && !targetIsTex;
  const abFeedbackActive = /giving feedback on a new version|which response do you prefer/i.test(
    normalizedText(document.body)
  );
  const candidates: DomTarCandidate[] = [];
  for (const element of controls) {
    const assistant = closestElement(element, '[data-message-author-role="assistant"]');
    if (
      (assistantRoots.length > 0 && !assistant)
      || closestElement(element, '[data-message-author-role="user"]')
      || closestElement(element, '[data-testid*="upload-chip"]')
    ) {
      continue;
    }

    const text = normalizedText(element);
    const href = getHref(element);
    const download = getAttr(element, 'download');
    const aria = getAttr(element, 'aria-label');
    const title = getAttr(element, 'title');
    const role = getAttr(element, 'role');
    const tagName = element.tagName.toLowerCase();
    const sourceValues = { text, href, download, aria, title };
    const tarSources = tarSourcesFor(sourceValues);
    const texSources = texSourcesFor(sourceValues);
    const artifactSources = artifactSourcesFor(sourceValues, comparableTarget);
    const clickable = isClickableControl(element);
    const visible = isVisible(element);
    if (
      abFeedbackActive
      && !href
      && !download
      && /^\s*[A-Za-z0-9][A-Za-z0-9._-]*\.tar(?:\(\d+\))?\.gz\s*$/i.test(text)
    ) {
      continue;
    }
    const genericArchiveDownload = Boolean(
      assistant
        && visible
        && hasGenericArchiveDownloadSignal(sourceValues)
    );
    const genericTexDownload = Boolean(
      targetIsTex
        && assistant
        && visible
        && hasGenericTexDownloadSignal(sourceValues)
    );
    const genericFileDownload = Boolean(
      targetIsGenericFile
        && assistant
        && visible
        && hasGenericFileDownloadSignal(sourceValues)
        && !hasConflictingArtifactName(sourceValues, comparableTarget)
        && !hasAnyArtifactName(sourceValues)
    );
    const hasCandidateSignal = targetIsGenericFile
      ? artifactSources.length > 0 || genericFileDownload
      : tarSources.length > 0 || texSources.length > 0 || genericArchiveDownload || genericTexDownload;
    if (!hasCandidateSignal || !clickable) {
      continue;
    }
    if (isDocumentTarLabelOnlyCandidate({ assistant, href, download, text, aria, title })) {
      continue;
    }

    let score = targetIsGenericFile
      ? artifactSources.length > 0 ? 260 : 120
      : texSources.length > 0 ? 260 : tarSources.length > 0 ? 200 : 120;
    if (/download/i.test(`${text} ${aria} ${title}`)) score += 100;
    if (tarNameLike(download)) score += 90;
    if (tarNameLike(href)) score += 80;
    if (tarNameLike(text)) score += 60;
    if (tarNameLike(`${aria} ${title}`)) score += 40;
    if (texNameLike(download)) score += 120;
    if (texNameLike(href)) score += 100;
    if (texNameLike(text)) score += 80;
    if (texNameLike(`${aria} ${title}`)) score += 60;
    if (genericArchiveDownload) score += 30;
    if (genericTexDownload) score += 80;
    if (genericFileDownload) score += 80;
    if (artifactSources.length > 0) score += 220;
    if (targetIsTex && texSources.length > 0) score += 200;
    if (targetIsTex && tarSources.length > 0) score -= 40;
    if (tagName === 'button' || role.toLowerCase() === 'button') score += 20;
    if (tagName === 'a') score += 10;
    if (assistant) score += 30;
    if (visible) score += 10;

    if (normalizedTarget) {
      const downloadMatch = normalizeArtifactComparable(download) === comparableTarget;
      const textMatch = normalizeArtifactComparable(text).includes(comparableTarget);
      const hrefMatch = normalizeArtifactComparable(href).includes(comparableTarget);
      const ariaTitleMatch = normalizeArtifactComparable(`${aria} ${title}`).includes(comparableTarget);
      const haystack = normalizeArtifactComparable(`${text} ${href} ${download} ${aria} ${title}`);
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
      visible,
      clickable,
      assistantIndex: assistant ? assistantRoots.indexOf(assistant as HTMLElement) : null,
      tarSources: tarSources.length > 0 ? tarSources : genericArchiveDownloadSources(sourceValues),
      artifactSources,
      fileKind: targetIsGenericFile || genericFileDownload
        ? 'downloaded-file'
        : texSources.length > 0 || genericTexDownload
          ? 'downloaded-tex'
          : tarSources.length > 0 || genericArchiveDownload
            ? 'downloaded-archive'
            : 'downloaded-file'
    });
  }
  return candidates.sort((left, right) => right.score - left.score || left.index - right.index);
}

function tarSourcesFor(values: Record<string, string>): string[] {
  return Object.entries(values)
    .filter(([, value]) => tarNameLike(value))
    .map(([name]) => name);
}

function texSourcesFor(values: Record<string, string>): string[] {
  return Object.entries(values)
    .filter(([, value]) => texNameLike(value))
    .map(([name]) => name);
}

function artifactSourcesFor(values: Record<string, string>, comparableTarget: string): string[] {
  if (!comparableTarget) {
    return [];
  }
  return Object.entries(values)
    .filter(([, value]) => normalizeArtifactComparable(value).includes(comparableTarget))
    .map(([name]) => name);
}

function hasGenericArchiveDownloadSignal(values: Record<string, string>): boolean {
  const haystack = Object.values(values).join(' ');
  return DOWNLOAD_ACTION_RE.test(haystack) && ARCHIVE_NOUN_RE.test(haystack);
}

function hasGenericTexDownloadSignal(values: Record<string, string>): boolean {
  const haystack = Object.values(values).join(' ');
  return DOWNLOAD_ACTION_RE.test(haystack) && TEX_NOUN_RE.test(haystack);
}

function hasGenericFileDownloadSignal(values: Record<string, string>): boolean {
  const haystack = Object.values(values).join(' ');
  return DOWNLOAD_ACTION_RE.test(haystack) && FILE_NOUN_RE.test(haystack);
}

function genericArchiveDownloadSources(values: Record<string, string>): string[] {
  return Object.entries(values)
    .filter(([, value]) => DOWNLOAD_ACTION_RE.test(value) || ARCHIVE_NOUN_RE.test(value))
    .map(([name]) => name);
}

function hasConflictingArtifactName(values: Record<string, string>, comparableTarget: string): boolean {
  if (!comparableTarget) {
    return false;
  }
  const names = artifactNamesFromValues(values)
    .map(normalizeArtifactComparable);
  return names.some((name) => name !== comparableTarget);
}

function hasAnyArtifactName(values: Record<string, string>): boolean {
  return artifactNamesFromValues(values).length > 0;
}

function artifactNamesFromValues(values: Record<string, string>): string[] {
  return Object.entries(values)
    .flatMap(([name, value]) => name === 'href' ? artifactNamesFromHref(value) : artifactNamesFromText(value));
}

function artifactNamesFromHref(value: string): string[] {
  try {
    const url = new URL(value, 'https://example.invalid/');
    const name = url.pathname.split('/').filter(Boolean).pop() || '';
    return name.includes('.') ? [name] : [];
  } catch {
    const name = String(value || '').split(/[/?#]/).filter(Boolean).pop() || '';
    return name.includes('.') ? [name] : [];
  }
}

function artifactNamesFromText(value: string): string[] {
  const names: string[] = [];
  const pattern = new RegExp(GENERIC_ARTIFACT_NAME_RE);
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(String(value || ''))) !== null) {
    if (match[1]) {
      names.push(match[1]);
    }
  }
  return names;
}

function isDocumentTarLabelOnlyCandidate(values: {
  assistant: Element | null | undefined;
  href: string;
  download: string;
  text: string;
  aria: string;
  title: string;
}): boolean {
  if (values.assistant != null) {
    return false;
  }
  if (tarNameLike(values.download) || tarNameLike(values.href)) {
    return false;
  }
  return tarNameLike(`${values.text} ${values.aria} ${values.title}`);
}

function tarNameLike(value: string): boolean {
  return TAR_NAME_RE.test(String(value || ''));
}

function texNameLike(value: string): boolean {
  return TEX_NAME_RE.test(String(value || ''));
}

function normalizeTarComparable(value: string): string {
  return normalizeArtifactComparable(value);
}

function normalizeArtifactComparable(value: string): string {
  return String(value || '')
    .replace(NORMALIZED_TAR_RE, '.tar.gz')
    .replace(/\.tgz/gi, '.tar.gz')
    .toLowerCase();
}

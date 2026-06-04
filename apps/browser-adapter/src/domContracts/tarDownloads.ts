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

function tarSourcesFor(values: Record<string, string>): string[] {
  return Object.entries(values)
    .filter(([, value]) => /\.tar\.gz/i.test(value))
    .map(([name]) => name);
}

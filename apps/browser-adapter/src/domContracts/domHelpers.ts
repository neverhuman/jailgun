export function queryAll<T extends Element>(root: ParentNode, selector: string): T[] {
  try {
    return Array.from(root.querySelectorAll<T>(selector));
  } catch {
    return [];
  }
}

export function getAttr(element: Element, name: string): string {
  return element.getAttribute(name) ?? '';
}

export function getHref(element: HTMLElement): string {
  return (element as HTMLAnchorElement).href || getAttr(element, 'href');
}

export function isVisible(element: HTMLElement): boolean {
  try {
    const view = element.ownerDocument.defaultView;
    const style = view?.getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style?.visibility !== 'hidden' && style?.display !== 'none' && rect.width >= 0 && rect.height >= 0;
  } catch {
    return true;
  }
}

export function isDisabled(element: HTMLElement): boolean {
  return element.hasAttribute('disabled') || /^true$/i.test(getAttr(element, 'aria-disabled'));
}

export function isClickableControl(element: HTMLElement): boolean {
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

export function closestElement(element: Element, selector: string): Element | undefined {
  let match: Element | null = null;
  try {
    match = element.closest(selector);
  } catch {
    match = null;
  }
  return match || void 0;
}

export function normalizedText(element: Element): string {
  return (element.textContent ?? '').replace(/\s+/g, ' ').trim();
}

export function bestLabel(element: HTMLElement): string {
  return normalizedText(element) || getAttr(element, 'aria-label') || getAttr(element, 'title');
}

export function surroundingContextText(element: HTMLElement): string {
  let node: HTMLElement | null = element.parentElement;
  const parts: string[] = [];
  for (let depth = 0; node && depth < 6; depth += 1) {
    parts.push(normalizedText(node));
    node = node.parentElement;
  }
  return parts.join(' ').replace(/\s+/g, ' ').trim().slice(0, 600);
}

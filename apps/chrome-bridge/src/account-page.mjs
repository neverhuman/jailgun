const COMPOSER = '[data-testid="prompt-textarea"],#prompt-textarea,textarea,[contenteditable="true"]';

function sameOrigin(page, origin) {
  try { return new URL(page.url()).origin === origin; } catch { return false; }
}

export async function selectAccountPage(context, previous, baseUrl) {
  const origin = new URL(baseUrl || 'https://chatgpt.com').origin;
  const pages = [...new Set([previous, ...context.pages()].filter(Boolean))]
    .filter((page) => !page.isClosed());
  const providerPages = pages.filter((page) => sameOrigin(page, origin));
  for (const page of providerPages) {
    if (await page.locator(COMPOSER).first().isVisible().catch(() => false)) return page;
  }
  if (previous && !previous.isClosed() && sameOrigin(previous, origin)) return previous;
  return providerPages[0] || pages[0] || context.newPage();
}

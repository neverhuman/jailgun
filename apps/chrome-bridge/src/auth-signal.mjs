/** Only identity fields leave the provider session response; never its tokens. */
export async function readAuthenticatedIdentity(page) {
  try {
    const identity = await page.evaluate(async () => {
      const response = await fetch('/api/auth/session', {
        credentials: 'same-origin',
        cache: 'no-store',
        signal: AbortSignal.timeout(3000),
      });
      if (!response.ok) return null;
      const body = await response.json();
      const id = body?.user?.id;
      const email = body?.user?.email;
      if (typeof id !== 'string' || !id.trim() || typeof email !== 'string' || !email.trim()) return null;
      return { id, email };
    });
    if (identity) return identity;
  } catch {
    // Unavailable/changed session signals must not turn an anonymous composer into readiness.
  }
  return readVisibleAccountIdentity(page);
}

export function uniqueEmailFromTexts(texts) {
  const emails = new Set(
    texts
      .flatMap((text) => String(text || '').match(/[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}/gi) || [])
      .map((email) => email.toLowerCase()),
  );
  return emails.size === 1 ? [...emails][0] : null;
}

/** Fall back to the visible account menu only; never search conversations or page storage. */
export async function readVisibleAccountIdentity(page) {
  if (typeof page?.locator !== 'function') return null;
  const controls = [
    '[data-testid="accounts-profile-button"]',
    '[data-testid="profile-button"]',
    'button[aria-label*="account" i]',
    'button[aria-label*="profile" i]',
  ];
  for (const selector of controls) {
    const control = page.locator(selector).first();
    if (!await control.isVisible().catch(() => false)) continue;
    try {
      await control.click({ timeout: 3000 });
      const surfaces = page.locator('[role="menu"]:visible,[role="dialog"]:visible,[data-radix-menu-content]:visible');
      await surfaces.first().waitFor({ state: 'visible', timeout: 3000 });
      const texts = await surfaces.evaluateAll((nodes) => nodes.map((node) => node.textContent || ''));
      const email = uniqueEmailFromTexts(texts);
      if (email) return { id: `email:${email}`, email };
    } catch {
      // Try another known account control. Anonymous pages remain unverified.
    } finally {
      await page.keyboard.press('Escape').catch(() => {});
    }
  }
  return null;
}

export function authenticatedComposerState({ composerDetected, identity, expectedEmail = '', loginVisible = false }) {
  if (!composerDetected) return null;
  if (!identity?.id || !identity?.email || loginVisible) {
    return { state: 'auth-required', reason: 'authenticated-account-signal-missing', identity: null };
  }
  if (expectedEmail && identity.email.trim().toLowerCase() !== expectedEmail.trim().toLowerCase()) {
    return { state: 'account-mismatch', reason: 'The signed-in ChatGPT account does not match the registered account.', identity: null };
  }
  return { state: 'ready', reason: null, identity };
}

/** Only identity fields leave the provider session response; never its tokens. */
export async function readAuthenticatedIdentity(page) {
  try {
    return await page.evaluate(async () => {
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
  } catch {
    // Unavailable/changed session signals must not turn an anonymous composer into readiness.
    return null;
  }
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

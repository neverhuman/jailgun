import { readAuthenticatedIdentity } from './auth-signal.mjs';
import { readChatGPTTextDom } from '../../browser-adapter/src/domContracts/chatgptText.mjs';

export class AdapterError extends Error {
  constructor(code, message, nextAction = 'Inspect the owned ChatGPT conversation before retrying.', details = {}) {
    super(message);
    this.name = 'AdapterError';
    this.code = code;
    this.nextAction = nextAction;
    this.retryAtMs = details.retry_at_ms ?? null;
    this.rateLimited = details.rate_limited === true;
  }
}

const wait = (ms, signal) => new Promise((resolve, reject) => {
  if (signal?.aborted) return reject(stopReason(signal));
  const cancel = () => { clearTimeout(timer); reject(stopReason(signal)); };
  const timer = setTimeout(() => { signal?.removeEventListener('abort', cancel); resolve(); }, ms);
  signal?.addEventListener('abort', cancel, { once: true });
});

function stopReason(signal) {
  return signal.reason instanceof AdapterError ? signal.reason : new AdapterError('cancelled', 'The capture was cancelled.');
}

function providerLimit(state, message, code = 'account-rate-limited') {
  const value = typeof state.retryAt === 'string' && /^\d{4}-\d\d-\d\dT\d\d:\d\d.*(?:Z|[+-]\d\d:\d\d)$/.test(state.retryAt) ? Date.parse(state.retryAt) : NaN;
  return new AdapterError(code, message, 'Wait for the indicated account cooldown, then verify readiness.', {
    rate_limited: true, retry_at_ms: Number.isSafeInteger(value) ? value : null,
  });
}

export function resolveModelLabel(requested, available) {
  const normalized = String(requested || '').trim().toLowerCase();
  if (!normalized) return null;
  const exact = available.find((label) => label.toLowerCase() === normalized);
  if (exact) return exact;
  if (!['astra', 'sol'].includes(normalized)) return null;
  const matches = available.filter((label) => label.toLowerCase().split(/[^a-z0-9]+/).includes(normalized));
  return matches.length === 1 ? matches[0] : null;
}

/** ChatGPT website operations only. Rust owns scheduling, persistence, budgets and browser lifetime. */
export class ChatGPTProvider {
  constructor(page, { identity, baseUrl = 'https://chatgpt.com', model = { mode: 'current' }, reasoningEffort = null, attachmentPath = null, pollMs = 300, settleMs = 1500, timeoutMs = 1800000 } = {}) {
    const base = new URL(baseUrl);
    if (base.origin !== 'https://chatgpt.com' && !(base.protocol === 'http:' && ['127.0.0.1', 'localhost'].includes(base.hostname))) {
      throw new AdapterError('provider-origin-invalid', 'The provider origin is not ChatGPT or a loopback test service.');
    }
    this.page = page;
    this.identity = identity;
    this.baseUrl = base.origin;
    this.model = model;
    this.reasoningEffort = reasoningEffort;
    this.attachmentPath = attachmentPath;
    this.pollMs = pollMs;
    this.settleMs = settleMs;
    this.timeoutMs = timeoutMs;
    this.prepared = null;
  }

  async verifyIdentity() {
    const identity = await readAuthenticatedIdentity(this.page);
    if (!identity) throw new AdapterError('authentication-expired', 'An authenticated account signal is required.', 'Reconnect the account in the operator dashboard.');
    if (this.identity && identity.email.toLowerCase() !== this.identity.email.toLowerCase()) {
      throw new AdapterError('account-mismatch', 'The page is signed in to a different account.', 'Reconnect the registered account.');
    }
    return this.identity ? { id: this.identity.id, email: identity.email } : identity;
  }

  async currentModel() {
    const state = await this.page.evaluate(readChatGPTTextDom);
    return !state.observedModel || state.observedModel === 'ChatGPT' ? 'current' : state.observedModel;
  }

  async checkReadiness() {
    const identity = await this.verifyIdentity();
    const state = await this.page.evaluate(readChatGPTTextDom);
    if (state.rateLimited) throw providerLimit(state, 'The account still reports a provider limit.');
    return { identity, model: await this.currentModel() };
  }

  async availableModels() {
    const control = this.page.locator('[data-testid="model-switcher-dropdown-button"],button[aria-label^="Model selector"],button[aria-label="Select ChatGPT model"]').first();
    if (!await control.isVisible().catch(() => false)) return [await this.currentModel()];
    await control.click({ timeout: 5000 });
    const items = this.page.locator('[role="menuitem"],[role="menuitemradio"]');
    try {
      await items.first().waitFor({ state: 'visible', timeout: 5000 });
      return await items.evaluateAll((items) => items
        .filter((item) => item.getAttribute('aria-disabled') !== 'true')
        .map((item) => (item.getAttribute('aria-label') || item.textContent || '').replace(/\s+/g, ' ').trim()).filter(Boolean));
    } finally {
      await this.page.keyboard.press('Escape');
      await items.first().waitFor({ state: 'hidden', timeout: 5000 }).catch(() => {});
    }
  }

  async selectModel() {
    const current = await this.currentModel();
    if (this.model.mode === 'current' || this.model.name === current) return current;
    const models = await this.availableModels();
    const selected = resolveModelLabel(this.model.name, models);
    if (!selected) throw new AdapterError('model-unavailable', 'The configured model is not available on this account.', 'Choose an observed available model or the current selection.');
    await this.page.locator('[data-testid="model-switcher-dropdown-button"],button[aria-label^="Model selector"],button[aria-label="Select ChatGPT model"]').first().click({ timeout: 5000 });
    const item = this.page.getByRole('menuitem', { name: selected, exact: true }).or(this.page.getByRole('menuitemradio', { name: selected, exact: true })).first();
    await item.click({ timeout: 5000 });
    const observed = await this.currentModel();
    if (observed !== 'current' && observed !== selected) throw new AdapterError('model-mismatch', 'The model selector did not confirm the requested model.');
    return observed === 'current' ? selected : observed;
  }

  async selectReasoningEffort() {
    if (!this.reasoningEffort) return { requested: null, applied: 'unchanged' };
    const requested = String(this.reasoningEffort).toLowerCase();
    const aliases = {
      low: ['low', 'light'],
      medium: ['medium', 'standard'],
      high: ['high', 'extended'],
      xhigh: ['xhigh', 'extra high', 'very high'],
      max: ['max', 'maximum'],
      ultra: ['ultra'],
    };
    if (!aliases[requested]) throw new AdapterError('reasoning-effort-invalid', 'The requested reasoning effort is invalid.');
    const control = this.page.locator('[data-testid="reasoning-effort-switcher"],button[aria-label^="Reasoning effort"],button[aria-label^="Thinking time"]').first();
    if (!await control.isVisible().catch(() => false)) {
      if (requested === 'medium') return { requested, applied: 'provider-default' };
      throw new AdapterError('reasoning-effort-unavailable', 'This browser model does not expose a reasoning-effort selector.', 'Use medium/provider-default or choose a model that exposes the requested effort.');
    }
    await control.click({ timeout: 5000 });
    const items = this.page.locator('[role="menuitem"],[role="menuitemradio"]');
    try {
      await items.first().waitFor({ state: 'visible', timeout: 5000 });
      const labels = await items.evaluateAll((nodes) => nodes.map((node) => (node.getAttribute('aria-label') || node.textContent || '').replace(/\s+/g, ' ').trim().toLowerCase()));
      const index = labels.findIndex((label) => aliases[requested].includes(label));
      if (index < 0) throw new AdapterError('reasoning-effort-unavailable', 'The requested reasoning effort is not available for this model.', 'Choose one of the effort labels exposed by the account browser.');
      await items.nth(index).click({ timeout: 5000 });
      return { requested, applied: labels[index] };
    } catch (error) {
      await this.page.keyboard.press('Escape').catch(() => {});
      throw error;
    }
  }

  async attachFile() {
    if (!this.attachmentPath) return;
    let input = this.page.locator('input[type="file"]').first();
    if (await input.count() === 0) {
      const attach = this.page.locator('[data-testid="composer-plus-btn"],button[aria-label*="Attach"],button[aria-label*="Upload"]').first();
      if (!await attach.isVisible().catch(() => false)) throw new AdapterError('attachment-control-missing', 'The ChatGPT attachment control is unavailable.');
      await attach.click({ timeout: 5000 });
      input = this.page.locator('input[type="file"]').first();
    }
    if (await input.count() === 0) throw new AdapterError('attachment-control-missing', 'The ChatGPT file input is unavailable.');
    await input.setInputFiles(this.attachmentPath, { timeout: 15000 });
  }

  async prepare(prompt) {
    if (typeof prompt !== 'string' || !prompt.trim()) throw new AdapterError('invalid-prompt', 'Prompt content is required.');
    await this.page.goto(this.baseUrl, { waitUntil: 'domcontentloaded', timeout: 45000 });
    await this.verifyIdentity();
    const composer = this.page.locator('#prompt-textarea,[data-testid="composer-text-input"],.ProseMirror[contenteditable="true"],[contenteditable="true"][role="textbox"][aria-label="Ask ChatGPT"]').first();
    try {
      await composer.waitFor({ state: 'visible', timeout: 30_000 });
    } catch {
      throw new AdapterError('adapter-composer-missing', 'The ChatGPT composer did not become available after navigation.');
    }
    const observedModel = await this.selectModel();
    await this.selectReasoningEffort();
    const state = await this.page.evaluate(readChatGPTTextDom);
    if (state.rateLimited) throw providerLimit(state, 'The account is rate limited.');
    await this.attachFile();
    await composer.fill(prompt, { timeout: 10000 });
    this.prepared = { prompt, beforeUserIds: state.userIds, observedModel };
    return { observed_model: observedModel };
  }

  /** Call only after Rust durably records submission-started and charges the budget. */
  async submit() {
    if (!this.prepared) throw new AdapterError('adapter-not-prepared', 'Prepare a prompt before submission.');
    const prepared = this.prepared;
    this.prepared = null; // A second call cannot accidentally send the same prompt.
    const send = this.page.locator('[data-testid="send-button"],button[aria-label="Send prompt"],button[aria-label="Send message"],button[aria-label="Send"]').first();
    if (!await send.isEnabled().catch(() => false)) throw new AdapterError('submission-not-accepted', 'The send control is unavailable or disabled.');
    try { await send.click({ timeout: 10000 }); }
    catch { throw new AdapterError('submission-uncertain', 'The send action did not return a definite result.'); }
    const deadline = Date.now() + 45000;
    while (Date.now() < deadline) {
      const state = await this.page.evaluate(readChatGPTTextDom, prepared);
      const url = new URL(this.page.url());
      const conversationId = url.pathname.match(/^\/c\/([^/]+)$/)?.[1];
      if (url.origin === this.baseUrl && conversationId && state.submittedUserId) {
        this.activeTurn = { conversation_id: conversationId, conversation_url: url.href, user_turn_id: state.submittedUserId, observed_model: state.observedModel || prepared.observedModel, accepted_at: new Date().toISOString() };
        return this.activeTurn;
      }
      if (state.rateLimited) throw providerLimit(state, 'A rate limit appeared before acceptance could be confirmed.', 'submission-uncertain');
      await wait(this.pollMs);
    }
    throw new AdapterError('submission-uncertain', 'The submitted user turn and conversation could not be verified.');
  }

  async capture(turn, { signal } = {}) {
    let expected = new URL(turn.conversation_url);
    let markdown = '';
    let lastContent = null;
    let stableSince = Date.now();
    let nextAuthCheck = 0;
    const deadline = Date.now() + this.timeoutMs;
    try {
      while (Date.now() < deadline) {
        if (signal?.aborted) throw stopReason(signal);
        const current = new URL(this.page.url());
        if (expected.origin !== this.baseUrl || current.origin !== expected.origin) throw new AdapterError('adapter-conversation-changed', 'The owned page navigated away from its conversation.');
        const expectedId = expected.pathname.match(/^\/c\/([^/]+)$/)?.[1];
        const currentId = current.pathname.match(/^\/c\/([^/]+)$/)?.[1];
        const canonicalizing = current.pathname !== expected.pathname
          && expectedId && currentId
          && decodeURIComponent(expectedId).startsWith('local-chatgpt:')
          && !decodeURIComponent(currentId).startsWith('local-chatgpt:');
        if (current.pathname !== expected.pathname && !canonicalizing) throw new AdapterError('adapter-conversation-changed', 'The owned page navigated away from its conversation.');
        const state = await this.page.evaluate(readChatGPTTextDom, { userTurnId: turn.user_turn_id });
        // ChatGPT initially assigns a local-chatgpt: route, then replaces it
        // with the server conversation id. Accept that one-way migration only
        // after the destination still proves ownership of the submitted turn.
        if (canonicalizing && state.error === 'adapter-turn-missing') {
          await wait(this.pollMs, signal);
          continue;
        }
        if (state.error) throw new AdapterError(state.error, 'The response cannot be associated with the accepted user turn.');
        if (canonicalizing) expected = current;
        markdown = state.markdown;
        if (state.rateLimited) throw providerLimit(state, 'Generation was interrupted by an account limit.');
        if (state.expired) throw new AdapterError('authentication-expired', 'The account session expired.');
        if (Date.now() >= nextAuthCheck) { await this.verifyIdentity(); nextAuthCheck = Date.now() + 15000; }
        if (state.observedModel && state.observedModel !== turn.observed_model) throw new AdapterError('model-mismatch', 'The observed model changed during generation.');
        const content = `${state.assistantId}\n${markdown}`;
        if (content !== lastContent || !state.completionSignal) { lastContent = content; stableSince = Date.now(); }
        if (state.completionSignal && Date.now() - stableSince >= this.settleMs) {
          if (!markdown.trim()) throw new AdapterError('empty-response', 'The completed response is empty.');
          return { ...turn, markdown, complete: true, error_code: null, completed_at: new Date().toISOString() };
        }
        await wait(this.pollMs, signal);
      }
      throw new AdapterError('response-timeout', 'The response did not complete within the configured timeout.');
    } catch (error) {
      return { ...turn, markdown, complete: false, error_code: error.code || 'adapter-browser-failure', retry_at_ms: error.retryAtMs ?? null, completed_at: new Date().toISOString() };
    }
  }

  async stop() {
    if (!this.activeTurn) return;
    const expected = new URL(this.activeTurn.conversation_url);
    const current = new URL(this.page.url());
    if (current.origin !== expected.origin || current.pathname !== expected.pathname) throw new AdapterError('ownership-mismatch', 'The page no longer shows the owned conversation.');
    const button = this.page.locator('[data-testid="stop-button"],button[aria-label="Stop generating"],button[aria-label="Stop"]').first();
    const deadline = Date.now() + 5000;
    while (Date.now() < deadline) {
      const state = await this.page.evaluate(readChatGPTTextDom, { userTurnId: this.activeTurn.user_turn_id });
      if (state.error) throw new AdapterError(state.error, 'The owned conversation cannot be checked for cancellation.');
      if (!state.streaming) return;
      if (await button.isVisible().catch(() => false)) {
        // Generation can finish while Playwright waits for a clickable Stop button.
        // A click timeout is not proof of failure: recheck the owned turn below.
        await button.click({ timeout: Math.min(1000, Math.max(1, deadline - Date.now())) }).catch(() => {});
      }
      await wait(this.pollMs);
    }
    throw new AdapterError('stop-unconfirmed', 'The owned conversation still appears to be generating.');
  }
}

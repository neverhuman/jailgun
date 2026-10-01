/** Self-contained DOM reader: Playwright serializes this function into the page. */
export function readChatGPTTextDom({ userTurnId = null, beforeUserIds = [], prompt = '' } = {}) {
  const visible = (node) => {
    if (!node) return false;
    const style = getComputedStyle(node);
    return style.display !== 'none' && style.visibility !== 'hidden';
  };
  const text = (node) => (node?.textContent ?? '').replace(/\s+/g, ' ').trim();
  const messageId = (node) => node.getAttribute('data-message-id')
    || (node.getAttribute('data-chatgpt-search-message-ids') ?? '').split(/\s+/).find(Boolean)
    || node.closest('[data-testid^="conversation-turn-"]')?.getAttribute('data-testid')
    || node.closest('[data-turn-key]')?.getAttribute('data-turn-key')
    || '';
  const messageRole = (node) => node.getAttribute('data-message-author-role')
    || (node.getAttribute('data-chatgpt-search-unit-key') ?? '').split(':').at(-1)
    || '';
  const legacyMessages = [...document.querySelectorAll('[data-message-author-role]')];
  const modernMessages = [...document.querySelectorAll('[data-chatgpt-search-unit-key][data-chatgpt-search-message-ids]')]
    .filter((node) => ['user', 'assistant'].includes(messageRole(node)));
  const messages = legacyMessages.length ? legacyMessages : modernMessages;
  const users = messages.filter((node) => messageRole(node) === 'user');
  const userIds = users.map(messageId).filter(Boolean);
  const submitted = users.find((node) => !beforeUserIds.includes(messageId(node)) && messageId(node) && text(node) === prompt.replace(/\s+/g, ' ').trim());
  const modelControl = [...document.querySelectorAll('[data-testid="model-switcher-dropdown-button"],button[aria-label^="Model selector"],button[aria-label="Select ChatGPT model"]')].find(visible);
  const observedModel = text(modelControl).replace(/^ChatGPT\s+/i, '').trim();
  const dialogs = [...document.querySelectorAll('[role="dialog"],[role="alert"],[aria-modal="true"]')].filter(visible);
  const rateLimit = dialogs.find((node) => /too many requests|making requests too quickly|temporarily limited access|reached.*limit|usage limit/i.test(text(node)));
  const expired = dialogs.some((node) => /session.*expired|log in again|sign in again/i.test(text(node)));
  const response = {
    userIds, submittedUserId: submitted ? messageId(submitted) : null, observedModel,
    rateLimited: Boolean(rateLimit), retryAt: rateLimit?.querySelector('time[datetime]')?.getAttribute('datetime') ?? null,
    expired, markdown: '', assistantId: null, completionSignal: false, streaming: false, error: null,
  };
  if (!userTurnId) return response;
  const index = messages.findIndex((node) => messageRole(node) === 'user' && messageId(node) === userTurnId);
  if (index < 0) return { ...response, error: 'adapter-turn-missing' };
  const following = messages.slice(index + 1);
  if (following.some((node) => messageRole(node) === 'user')) return { ...response, error: 'adapter-turn-changed' };
  const assistantMessages = following.filter((node) => messageRole(node) === 'assistant');
  if (!assistantMessages.length) return response;
  // Multiple assistant message nodes may represent tools/reasoning. Require an unambiguous final message.
  const answer = assistantMessages.at(-1);
  const id = messageId(answer);
  if (!id) return { ...response, error: 'adapter-message-id-missing' };
  const content = answer.querySelector('[data-markdown-text-style="assistant-message"],.markdown') || answer;

  const escape = (value) => value.replace(/([\\`*_{}\[\]<>])/g, '\\$1');
  const inline = (node) => [...node.childNodes].map(convert).join('');
  function convert(node) {
    if (node.nodeType === 3) return escape(node.textContent ?? '');
    if (node.nodeType !== 1) return '';
    const tag = node.tagName.toLowerCase();
    if (['script', 'style', 'button', 'svg', 'noscript'].includes(tag) || node.getAttribute('aria-hidden') === 'true') return '';
    if (tag === 'pre') {
      const code = node.querySelector('code') || node;
      const body = code.textContent ?? '';
      const language = (code.className?.match(/language-([\w+-]+)/) ?? [])[1] ?? '';
      const fence = '`'.repeat(Math.max(3, ...[...body.matchAll(/`+/g)].map((match) => match[0].length + 1)));
      return `\n\n${fence}${language}\n${body.replace(/\n$/, '')}\n${fence}\n\n`;
    }
    if (tag === 'code') {
      const body = node.textContent ?? '';
      const fence = '`'.repeat(Math.max(1, ...[...body.matchAll(/`+/g)].map((match) => match[0].length + 1)));
      return `${fence} ${body} ${fence}`;
    }
    if (/^h[1-6]$/.test(tag)) return `\n\n${'#'.repeat(Number(tag[1]))} ${inline(node).trim()}\n\n`;
    if (tag === 'br') return '  \n';
    if (tag === 'hr') return '\n\n---\n\n';
    if (tag === 'strong' || tag === 'b') return `**${inline(node)}**`;
    if (tag === 'em' || tag === 'i') return `*${inline(node)}*`;
    if (tag === 'a') {
      const href = node.getAttribute('href') ?? '';
      if (!/^https?:\/\//i.test(href) && !href.startsWith('/') && !href.startsWith('#')) return inline(node);
      const destination = href.replace(/\s/g, '%20').replace(/\(/g, '%28').replace(/\)/g, '%29');
      return `[${inline(node)}](${destination})`;
    }
    if (tag === 'ul' || tag === 'ol') {
      const start = Number(node.getAttribute('start') || 1);
      const items = [...node.children].filter((child) => child.tagName.toLowerCase() === 'li');
      return '\n\n' + items.map((item, position) => {
        const prefix = tag === 'ol' ? `${start + position}. ` : '- ';
        return prefix + inline(item).trim().replace(/\n/g, '\n' + ' '.repeat(prefix.length));
      }).join('\n') + '\n\n';
    }
    if (tag === 'blockquote') return '\n\n' + inline(node).trim().split('\n').map((line) => `> ${line}`).join('\n') + '\n\n';
    if (tag === 'table') {
      const rows = [...node.querySelectorAll('tr')].map((row) => [...row.children].map((cell) => inline(cell).trim().replace(/\|/g, '\\|').replace(/\n/g, '<br>')));
      if (!rows.length) return '';
      const rendered = rows.map((row) => `| ${row.join(' | ')} |`);
      rendered.splice(1, 0, `| ${rows[0].map(() => '---').join(' | ')} |`);
      return '\n\n' + rendered.join('\n') + '\n\n';
    }
    if (tag === 'p' || tag === 'div' || tag === 'section') return `\n\n${inline(node)}\n\n`;
    return inline(node);
  }
  const markdown = inline(content).trim();
  const turn = answer.closest('[data-testid^="conversation-turn-"],[data-turn-key],article') || answer;
  const copy = [...turn.querySelectorAll('button')].some((button) => visible(button) && /^(copy|copy response)$/i.test(button.getAttribute('aria-label') ?? ''));
  const stopping = [...document.querySelectorAll('[data-testid="stop-button"],button[aria-label="Stop generating"],button[aria-label="Stop"]')].some(visible);
  const busy = answer.matches('[aria-busy="true"],[data-is-streaming="true"]') || Boolean(answer.querySelector('[aria-busy="true"],[data-is-streaming="true"]'));
  return { ...response, markdown, assistantId: id, completionSignal: copy && !stopping && !busy, streaming: stopping || busy };
}

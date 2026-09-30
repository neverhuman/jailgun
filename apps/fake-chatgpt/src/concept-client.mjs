const modelButton = document.querySelector('[data-testid="model-switcher-dropdown-button"]');
const stopButton = document.querySelector('[data-testid="stop-button"]');
const sendButton = document.querySelector('[data-testid="send-button"]');
const composer = document.querySelector('#prompt-textarea');
let model = localStorage.getItem('fixture-model') || 'Fixture One';
let models = [];
modelButton.textContent = model;
let conversationId = location.pathname.match(/^\/c\/([^/]+)$/)?.[1];
let polling = false;

async function session() {
  const result = await fetch('/api/auth/session').then((response) => response.json());
  document.querySelector('#login').hidden = Boolean(result.user);
  models = result.models ?? [];
}
document.querySelector('#login').onclick = async () => { await fetch('/api/test-login', { method: 'POST' }); await session(); };
modelButton.onclick = () => {
  const menu = document.querySelector('#models');menu.replaceChildren();menu.setAttribute('role', 'menu');
  for (const name of models) {
    const item = document.createElement('button');item.setAttribute('role', 'menuitem');item.textContent = name;
    item.onclick = () => { model = name;modelButton.textContent = model;localStorage.setItem('fixture-model', model);menu.replaceChildren(); };
    menu.append(item);
  }
};
document.addEventListener('keydown', (event) => { if (event.key === 'Escape') document.querySelector('#models').replaceChildren(); });
document.querySelector('#composer').onsubmit = async (event) => {
  event.preventDefault();sendButton.disabled = true;
  try {
    const response = await fetch('/api/conversation', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ prompt: composer.value, model }) });
    const result = await response.json();
    if (!response.ok) { showAlert(result.error === 'account-rate-limited', result.error === 'authentication-expired', result.retryAt);return; }
    conversationId = result.id;history.pushState({}, '', `/c/${conversationId}`);composer.value = '';await refresh();
  } finally { sendButton.disabled = false; }
};
stopButton.onclick = async () => { await fetch(`/api/conversation/${conversationId}/stop`, { method: 'POST' });await refresh(); };

async function refresh() {
  if (!conversationId || polling) return;
  polling = true;
  try {
    const response = await fetch(`/api/conversation/${conversationId}`);
    if (!response.ok) return;
    const state = await response.json();
    const root = document.querySelector('#conversation');root.replaceChildren();
    const user = document.createElement('article');user.setAttribute('data-message-author-role', 'user');user.setAttribute('data-message-id', state.userId);user.textContent = state.prompt;root.append(user);
    const assistant = document.createElement('article');assistant.setAttribute('data-message-author-role', 'assistant');assistant.setAttribute('data-message-id', state.assistantId);assistant.setAttribute('aria-busy', String(state.generating));
    const content = document.createElement('div');content.className = 'markdown';assistant.append(content);
    for (const block of state.blocks) {
      const text = block.text.slice(0, Math.ceil(block.text.length * state.progress));
      if (!text) continue;
      let element = document.createElement(block.tag);
      if (block.tag === 'pre') {
        const code = document.createElement('code');code.className = `language-${block.language}`;code.textContent = text;element.append(code);
      } else { element.textContent = text; }
      if (block.tag === 'a') element.href = block.href;
      if (block.tag === 'li') { const list = document.createElement('ul');list.append(element);element = list; }
      content.append(element);
    }
    if (state.complete) {
      const toolbar = document.createElement('div');toolbar.setAttribute('role', 'toolbar');
      const copy = document.createElement('button');copy.setAttribute('aria-label', 'Copy response');copy.textContent = 'Copy response';toolbar.append(copy);assistant.append(toolbar);
    }
    root.append(assistant);stopButton.hidden = !state.generating;
    showAlert(state.rateLimited, state.expired, state.retryAt);
  } finally { polling = false; }
}
function showAlert(rateLimited, expired, retryAt) {
  document.querySelector('#alerts').replaceChildren();
  if (rateLimited || expired) {
    const alert = document.createElement('div');alert.setAttribute('role', 'alert');alert.textContent = expired ? 'Your session expired. Sign in again.' : 'Too many requests. Please wait a few minutes.';document.querySelector('#alerts').append(alert);
    if (rateLimited && retryAt) { const time = document.createElement('time');time.dateTime = retryAt;time.textContent = retryAt;alert.append(time); }
  }
}
await session();await refresh();setInterval(refresh, 50);

import assert from 'node:assert/strict';
import test from 'node:test';
import { JSDOM } from 'jsdom';
import { readChatGPTTextDom } from '../../browser-adapter/src/domContracts/chatgptText.mjs';

function inspect(html, args = { userTurnId: 'submitted' }) {
  const dom = new JSDOM(html, { runScripts: 'outside-only' });
  try { return dom.window.eval(`(${readChatGPTTextDom.toString()})(${JSON.stringify(args)})`); }
  finally { dom.window.close(); }
}

const user = '<article data-message-author-role="user" data-message-id="submitted">Synthetic prompt</article>';
const response = (body, extra = '') => `<article data-message-author-role="assistant" data-message-id="answer" ${extra}><div class="markdown">${body}</div><button aria-label="Copy response">Copy</button></article>`;

test('captures the submitted turn and excludes prior responses and document text', () => {
  const result = inspect('<p>Unrelated page content</p>' + response('<p>Previous response</p>') + user + response('<h2>Current answer</h2><p>Details</p>'));
  assert.match(result.markdown, /^## Current answer/);
  assert.doesNotMatch(result.markdown, /Previous|Unrelated/);
  assert.equal(result.completionSignal, true);
});

test('new user messages and missing IDs cannot select another response', () => {
  assert.equal(inspect(user + response('Answer') + '<article data-message-author-role="user" data-message-id="different">Different prompt</article>').error, 'adapter-turn-changed');
  assert.equal(inspect('<article data-message-author-role="user">No ID</article>' + response('Unrelated')).error, 'adapter-turn-missing');
  assert.equal(inspect(user + '<article data-message-author-role="assistant">No ID</article>').error, 'adapter-message-id-missing');
});

test('streaming indicators prevent completion despite visible copy controls', () => {
  assert.equal(inspect(user + response('Still streaming', 'aria-busy="true"')).completionSignal, false);
  assert.equal(inspect(user + response('Still streaming') + '<button data-testid="stop-button">Stop</button>').completionSignal, false);
  assert.equal(inspect(user + '<article data-message-author-role="assistant" data-message-id="answer"><p>No completion signal</p></article>').completionSignal, false);
});

test('compact thinking-effort controls are not reported as model names', () => {
  const result = inspect('<button aria-label="Select ChatGPT model">Thinking effort5.6 Medium</button>', { userTurnId: null });
  assert.equal(result.observedModel, '');
});

test('Markdown preserves nested lists and fenced code and excludes executable HTML', () => {
  const body = '<h1>Proposal</h1><ol start="3"><li>Third<ul><li>Nested</li></ul></li></ol><pre><code class="language-js">const code = "```";\n</code></pre><p><strong>Bold</strong> <em>Emphasis</em> <a href="https://example.invalid/a(b)">Link</a><a href="javascript:alert(1)">Unsafe destination</a></p><script>alert(1)</script>';
  const result = inspect(user + response(body));
  assert.match(result.markdown, /3\. Third/);
  assert.match(result.markdown, /- Nested/);
  assert.match(result.markdown, /````js\nconst code = "```";/);
  assert.match(result.markdown, /\*\*Bold\*\*/);
  assert.match(result.markdown, /a%28b%29/);
  assert.doesNotMatch(result.markdown, /javascript:|<script>|alert\(1\)/);
});

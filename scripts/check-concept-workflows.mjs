import { spawn, execFileSync } from 'node:child_process';
import { randomUUID, createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { chromium } from 'playwright-core';
import { start } from '../apps/fake-chatgpt/src/server.mjs';

const service = await start({ port: 0, interactive: true });
service.concept.controls.generationMs = 4500;
service.concept.controls.completionDelayMs = 300;
const publishConversation = service.concept.conversations.set;
let heldIsolationVictims = null;
service.concept.conversations.set = function (id, entry) {
  // Cancellation must observe an in-progress victim even if the separate main
  // run takes longer than the fixture's normal completion timer to start.
  // Hold only that synthetic victim, before its conversation is published.
  if (heldIsolationVictims && entry.prompt.includes('A synthetic concept whose work will be cancelled')) {
    entry.mode = 'partial';
    heldIsolationVictims.add(id);
  }
  return publishConversation.call(this, id, entry);
};
const chrome = process.env.JAILGUN_TEST_CHROME || (existsSync('/usr/bin/google-chrome') ? '/usr/bin/google-chrome' : chromium.executablePath());
try {
  for (const [count, scenario] of [[5, 'complete'], [10, 'complete'], [5, 'recovery'], [5, 'isolation'], [5, 'run-deadline'], [5, 'response-timeout'], [5, 'rate-limit'], [5, 'rate-limit-uncertain'], [5, 'account-http'], [5, 'reconnect'], [5, 'mcp-http'], [10, 'mcp-http'], [5, 'mcp-stdio'], [10, 'mcp-stdio'], [5, 'cli'], [10, 'cli'], [5, 'two-accounts']]) {
    if (process.argv[2] && process.argv[2] !== scenario) continue;
    if (!process.argv[2] && ['reconnect', 'two-accounts'].includes(scenario)) continue; // Owned by registered Rust browser integration tests.
    service.concept.controls.models = ['Fixture One', 'Fixture Two'];
    service.concept.controls.mode = ['run-deadline', 'response-timeout'].includes(scenario) ? 'partial' : scenario.startsWith('rate-limit') ? scenario : 'complete';
    service.concept.controls.retryAt = scenario.startsWith('rate-limit') ? new Date(2_000_000).toISOString() : null;
    heldIsolationVictims = scenario === 'isolation' ? new Set() : null;
    const previousConversations = new Set(service.concept.conversations.keys());
    const previousSubmissions = service.concept.submissions.length;
    const accountProof = scenario === 'two-accounts' || scenario === 'cli' || scenario === 'reconnect' || scenario === 'account-http' || scenario.startsWith('mcp-');
    const binary = accountProof ? 'target/debug/examples/account_browser_proof' : 'target/debug/examples/concept_browser_proof';
    const output = resolve('target/concept-workflows', `${count}-${scenario}-${randomUUID()}`);
    await mkdir(output, { recursive: true, mode: 0o700 });
    await new Promise((resolveProof, reject) => {
      const child = spawn(resolve(binary), [service.url, output, chrome, String(count), accountProof && scenario === 'account-http' ? 'http' : scenario], { stdio: ['ignore', 'inherit', 'inherit'] });
      const deadline = setTimeout(() => {
        child.kill('SIGTERM');
        const force = setTimeout(() => child.kill('SIGKILL'), 5000);force.unref();
        reject(new Error(`concept workflow proof timed out: ${scenario}`));
      }, 240_000);
      child.on('error', error => { clearTimeout(deadline);reject(error); });
      child.on('exit', (code, signal) => { clearTimeout(deadline);code === 0 ? resolveProof() : reject(new Error(`concept workflow proof failed: ${code ?? signal}`)); });
    });
    if (['run-deadline', 'response-timeout', 'rate-limit'].includes(scenario)) {
      const conversations = [...service.concept.conversations.values()].filter(entry => !previousConversations.has(entry.id));
      if (conversations.length !== 1 || conversations[0].mode !== 'stopped') throw new Error('execution limit did not stop exactly the owned conversation');
    }
    if (heldIsolationVictims && (!heldIsolationVictims.size || [...heldIsolationVictims].some(id => service.concept.conversations.get(id)?.mode !== 'stopped'))) throw new Error('isolation proof did not stop every held victim conversation');
    if (scenario === 'rate-limit-uncertain' && (service.concept.submissions.length !== previousSubmissions + 1 || service.concept.conversations.size !== previousConversations.size)) throw new Error('uncertain submission was retried or misidentified as accepted');
    if (scenario === 'two-accounts') {
      const submissions = service.concept.submissions.slice(previousSubmissions);
      if (submissions.length !== 9 || submissions.some(entry => entry.accountId !== 'provider-synthetic')) throw new Error('two-account work used the wrong provider identity or repeated submissions');
    }
    const evidencePath = resolve(output, 'evidence.json');
    const evidence = JSON.parse(await readFile(evidencePath, 'utf8'));
    const files = execFileSync('git', ['ls-files', '--cached', '--others', '--exclude-standard', '-z'], { encoding: 'utf8' }).split('\0').filter(Boolean);
    const inputs = files.filter((file) => file.startsWith('crates/jailgun-workflow/') || file.startsWith('crates/jailgun-orchestrator/') || file.startsWith('crates/jailgun-server/') || file.startsWith('crates/jailgun-cli/') || file.startsWith('apps/chrome-bridge/') || file.startsWith('apps/browser-adapter/') || file.startsWith('apps/fake-chatgpt/') || file.startsWith('db/migrations/') || file === 'Cargo.lock' || file === 'scripts/check-concept-workflows.mjs');
    const hashes = {};
    for (const file of inputs) hashes[file] = createHash('sha256').update(await readFile(file)).digest('hex');
    Object.assign(evidence, {
      generated_at: new Date().toISOString(), scenario, command: [binary, '<loopback-fixture>', '<private-output>', '<chrome>', String(count), scenario === 'account-http' ? 'http' : scenario], exit_status: 0,
      commit: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(), tree: execFileSync('git', ['rev-parse', 'HEAD^{tree}'], { encoding: 'utf8' }).trim(),
      dirty: Boolean(execFileSync('git', ['status', '--porcelain'], { encoding: 'utf8' }).trim()), input_sha256: hashes,
      binary_sha256: createHash('sha256').update(await readFile(binary)).digest('hex'),
      ...(scenario === 'cli' ? { cli_binary_sha256: createHash('sha256').update(await readFile('target/debug/jailgun')).digest('hex') } : {}),
      ...(scenario === 'mcp-stdio' ? { stdio_binary_sha256: createHash('sha256').update(await readFile('target/debug/jailgun')).digest('hex'), stderr_sha256: createHash('sha256').update(await readFile(resolve(output, 'stdio-stderr.log'))).digest('hex') } : {}),
      ...(heldIsolationVictims ? { cancellation_fixture: { held_provider_ids: [...heldIsolationVictims], mode_before_cancellation: 'partial', all_held_victims_stopped: true } } : {}),
      node: process.version, chrome: execFileSync(chrome, ['--version'], { encoding: 'utf8' }).trim(), chromium_sandbox: true,
    });
    await writeFile(evidencePath, JSON.stringify(evidence, null, 2) + '\n', { mode: 0o600 });
    console.log(`Synthetic ${count}-candidate ${scenario} workflow evidence: ${output}/evidence.json`);
  }
} finally {
  service.concept.conversations.set = publishConversation;
  await service.stop();
}

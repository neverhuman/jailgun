import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, readFile, stat, writeFile } from 'node:fs/promises';
import { dirname, resolve, relative, isAbsolute } from 'node:path';

const git = (...args) => execFileSync('git', args, { encoding: 'utf8' }).trim();
const root = resolve('.');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const files = [...new Set(git('ls-files', '--cached', '--others', '--exclude-standard', '-z').split('\0'))]
  .filter(path => path.endsWith('.md') && !path.startsWith('agent/') && !path.startsWith('.jankurai/')).sort();
const inputs = {};
const failures = [];
let links = 0, examples = 0;
for (const path of files) {
  const bytes = await readFile(path);inputs[path] = hash(bytes);
  const text = bytes.toString('utf8');
  for (const match of text.matchAll(/^```json\s*\n([\s\S]*?)^```\s*$/gm)) {
    examples += 1;
    try { JSON.parse(match[1]); } catch { failures.push(`${path}: invalid fenced JSON example`); }
  }
  const prose = text.replace(/^(`{3,}|~{3,})[^\n]*\n[\s\S]*?^\1\s*$/gm, '').replace(/`[^`\n]*`/g, '');
  const targets = [...prose.matchAll(/!?\[[^\]\n]*\]\((<[^>]+>|[^\s)]+)(?:\s+"[^"]*")?\)/g)].map(m => m[1].replace(/^<|>$/g, ''));
  targets.push(...[...prose.matchAll(/<(?:img|a)\s[^>]*(?:src|href)="([^"]+)"/g)].map(m => m[1]));
  for (const target of targets) {
    if (/^(?:[a-z][a-z0-9+.-]*:|#)/i.test(target)) continue;
    const file = decodeURIComponent(target.split(/[?#]/)[0]);
    const absolute = resolve(dirname(resolve(path)), file);
    const local = relative(root, absolute);
    if (isAbsolute(file) || local === '..' || local.startsWith('../')) {
      failures.push(`${path}: local link leaves the repository: ${target}`);continue;
    }
    try {
      const metadata = await stat(absolute);
      assert.ok(metadata.isFile() || metadata.isDirectory());
      if (metadata.isFile()) inputs[local] = hash(await readFile(absolute));
      links += 1;
    } catch { failures.push(`${path}: missing local link: ${target}`); }
  }
}
const commands = [];
try {
  const record = JSON.parse(await readFile('assets/dashboard-results.json', 'utf8'));
  assert.equal(record.screenshot, 'assets/dashboard-results.png');
  assert.equal(hash(await readFile(record.screenshot)), record.sha256);
  for (const { path, sha256 } of record.source.inputs) {
    assert.equal(hash(await readFile(path)), sha256, `Stale screenshot input: ${path}`);
  }
} catch (error) { failures.push(`Dashboard screenshot provenance: ${error.message}`); }
const surfaces = ['setup', 'doctor', 'uninstall', 'accounts list', 'accounts connect', 'accounts reconnect', 'brainstorm',
  'runs list', 'runs show', 'runs result', 'runs pause', 'runs resume', 'runs cancel', 'runs export',
  'data backup', 'data restore', 'service status', 'service stop', 'service unit', 'mcp', 'worker', 'serve', 'validate-config', 'run', 'deploy-archive',
  'remote-cleanup', 'tar-validate', 'telegram-send', 'notify-commit'];
for (const [name, args] of [...surfaces.map(command => ['jailgun', command.split(' ')]), ['jailhard', []]]) {
  const binary = resolve(`target/debug/${name}`);
  inputs[`target/debug/${name}`] ??= hash(await readFile(binary));
  const argv = [...args, '--help'];
  try {
    const output = execFileSync(binary, argv, { timeout: 10_000, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
    assert.match(output, /Usage:/);
    commands.push({ binary: name, args: argv, exit_status: 0, output_sha256: hash(output) });
  } catch { failures.push(`${name} ${argv.join(' ')}: help command failed`); }
}
for (const path of ['scripts/check-docs.mjs', 'ops/ci/docs.sh']) inputs[path] = hash(await readFile(path));
await mkdir('target/jankurai/docs', { recursive: true });
await writeFile('target/jankurai/docs/evidence.json', `${JSON.stringify({
  generated_at: new Date().toISOString(), command: 'bash ops/ci/docs.sh',
  status: failures.length ? 'fail' : 'pass', exit_status: failures.length ? 1 : 0,
  scope: 'local Markdown file links, JSON syntax and executable CLI help surfaces; excludes generated audit reports; does not verify remote links, anchors or clean-clone/live-provider acceptance',
  source: { commit: git('rev-parse', 'HEAD'), tree: git('rev-parse', 'HEAD^{tree}'), dirty: Boolean(git('status', '--porcelain')), input_sha256: inputs },
  node: process.version, markdown_files: files.length, local_links: links, json_examples: examples, commands, failures,
}, null, 2)}\n`);
if (failures.length) { console.error(failures.join('\n'));process.exitCode = 1; }
else console.log(`Documentation checks passed: ${files.length} Markdown files, ${links} local links, ${examples} JSON examples, ${commands.length} CLI help surfaces`);

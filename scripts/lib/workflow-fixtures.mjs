import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';

// Exercise actual SQLite transitions, then normalize only generated identities.
// No browser profile, credential, runtime path or live transcript is exported.
export function workflowFixtures(root) {
  const raw = JSON.parse(execFileSync('cargo', ['run', '--quiet', '--locked', '-p', 'jailgun-workflow', '--example', 'dashboard_fixtures'], {
    cwd: root, encoding: 'utf8', maxBuffer: 8 * 1024 * 1024,
    env: { ...process.env, CARGO_INCREMENTAL: process.env.CARGO_INCREMENTAL || '0', CARGO_PROFILE_DEV_DEBUG: process.env.CARGO_PROFILE_DEV_DEBUG || 'line-tables-only' },
  }));
  const ids = new Map();
  function register(value) { if (!ids.has(value)) ids.set(value, `00000000-0000-4000-8000-${String(ids.size + 1).padStart(12, '0')}`); }
  const run = raw['run-completed'];
  register(run.id);
  for (const task of run.tasks) register(task.id);
  for (const attempt of raw['completed-attempts']) register(attempt.id);
  for (const artifact of [...run.artifacts].sort(artifactOrder)) register(artifact.id);
  for (const event of raw.events) if (event.data?.reservation_id) register(event.data.reservation_id);
  let text = JSON.stringify(raw);
  for (const [original, replacement] of ids) text = text.replaceAll(original, replacement);
  const normalized = JSON.parse(text);
  const final = normalized['run-completed'].artifacts.find(artifact => artifact.name === 'result.json');
  const manifest = JSON.parse(normalized.artifacts[final.id]);
  manifest.artifacts.sort(artifactOrder);
  const manifestText = JSON.stringify(manifest, null, 2);
  normalized.artifacts[final.id] = manifestText;
  const digest = createHash('sha256').update(manifestText).digest('hex');
  function ordered(value) {
    if (Array.isArray(value)) return (value.every(v => v && typeof v === 'object' && 'sha256' in v && 'name' in v) ? [...value].sort(artifactOrder) : value).map(ordered);
    if (!value || typeof value !== 'object') return value;
    if (value.id === final.id && value.name === 'result.json') value = { ...value, sha256: digest, byte_length: Buffer.byteLength(manifestText) };
    return Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b, 'en')).map(([key, child]) => [key, ordered(child)]));
  }
  return ordered(normalized);
}
function artifactOrder(a, b) { return a.created_ms - b.created_ms || a.name.localeCompare(b.name, 'en'); }

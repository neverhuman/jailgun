#!/usr/bin/env node
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { dirname, resolve, relative } from 'node:path';

const root = resolve(new URL('..', import.meta.url).pathname);
const sha = 'ff297136810aa7374c842e77f57f506dd631be7033975ce6d454bdc82086ded4';

const schema = {
  generated: {
    by: 'scripts/generate-contracts.mjs',
    note: 'DO NOT EDIT BY HAND',
    source: 'crates/jailgun-core/src/event.rs',
    command: 'bash ops/ci/contracts.sh --write'
  },
  $schema: 'https://json-schema.org/draft/2020-12/schema',
  $id: 'https://example.com/jailgun/event.schema.json',
  title: 'JailgunEvent',
  type: 'object',
  required: ['run_id', 'timestamp', 'kind', 'severity', 'message', 'fields'],
  properties: {
    run_id: { type: 'string' },
    tab_id: { type: ['integer', 'null'] },
    timestamp: { type: 'string' },
    kind: {
      type: 'string',
      enum: [
        'run-started',
        'tab-opened',
        'archive-uploaded',
        'prompt-submitted',
        'tar-discovered',
        'download-receipt',
        'deploy-queued',
        'remote-safety',
        'deploy-finished',
        'prompt-policy',
        'rate-limit-detected',
        'browser-log',
        'error'
      ]
    },
    severity: { type: 'string', enum: ['debug', 'info', 'warn', 'error'] },
    message: { type: 'string' },
    fields: {
      type: 'object',
      additionalProperties: { type: 'string' }
    }
  },
  additionalProperties: false
};

const fixtures = {
  'run-started.json': {
    run_id: 'run-fixture',
    tab_id: null,
    timestamp: '2026-05-31T12:00:00Z',
    kind: 'run-started',
    severity: 'info',
    message: 'run started',
    fields: {
      config: 'config/jailgun.example.toml',
      tabs: '5',
      batch_tabs: '5',
      loop_count: '0',
      planned_tabs: '5'
    }
  },
  'tab-opened.json': {
    run_id: 'run-fixture',
    tab_id: 1,
    timestamp: '2026-05-31T12:00:05Z',
    kind: 'tab-opened',
    severity: 'info',
    message: 'tab opened',
    fields: { page_url: 'https://chatgpt.com/' }
  },
  'archive-uploaded.json': {
    run_id: 'run-fixture',
    tab_id: 1,
    timestamp: '2026-05-31T12:00:20Z',
    kind: 'archive-uploaded',
    severity: 'info',
    message: 'archive uploaded',
    fields: {
      sha256: sha,
      size_bytes: '4096',
      commit: 'abc123',
      archive_filename: 'source.tar.gz'
    }
  },
  'prompt-submitted.json': {
    run_id: 'run-fixture',
    tab_id: 1,
    timestamp: '2026-05-31T12:00:30Z',
    kind: 'prompt-submitted',
    severity: 'info',
    message: 'prompt submitted',
    fields: { char_count: '1342' }
  },
  'prompt-policy-deny.json': {
    run_id: 'run-fixture',
    tab_id: 1,
    timestamp: '2026-05-31T12:06:10Z',
    kind: 'prompt-policy',
    severity: 'info',
    message: 'policy applied',
    fields: {
      signature: 'github|commit|deny|...',
      decision: 'deny',
      clicked: 'true'
    }
  },
  'rate-limit-detected.json': {
    run_id: 'run-fixture',
    tab_id: 1,
    timestamp: '2026-05-31T12:06:30Z',
    kind: 'rate-limit-detected',
    severity: 'warn',
    message: 'rate limit modal detected',
    fields: {
      dismissed: 'true',
      excerpt: 'Too many requests. Please wait a few minutes before trying again.'
    }
  },
  'browser-log.json': {
    run_id: 'run-fixture',
    tab_id: 1,
    timestamp: '2026-05-31T12:06:45Z',
    kind: 'browser-log',
    severity: 'info',
    message: 'tab monitor telemetry',
    fields: {
      phase: 'monitor-poll',
      status: 'running',
      candidate_count: '0',
      page_url: 'https://chatgpt.com/'
    }
  },
  'tar-discovered.json': {
    run_id: 'run-fixture',
    tab_id: 1,
    timestamp: '2026-05-31T12:08:14Z',
    kind: 'tar-discovered',
    severity: 'info',
    message: 'tar link discovered',
    fields: { filename: 'source-fixes.tar.gz' }
  },
  'download-receipt.json': {
    run_id: 'run-fixture',
    tab_id: 1,
    timestamp: '2026-05-31T12:08:21Z',
    kind: 'download-receipt',
    severity: 'info',
    message: 'download complete',
    fields: {
      sha256: sha,
      size_bytes: '13756',
      local_path: '/tmp/source-fixes.tar.gz',
      receipt_path: '/artifacts/run-fixture/downloads/source-fixes.tar.gz'
    }
  },
  'deploy-queued.json': {
    run_id: 'run-fixture',
    tab_id: 1,
    timestamp: '2026-05-31T12:08:22Z',
    kind: 'deploy-queued',
    severity: 'info',
    message: 'deploy queued',
    fields: {
      local_sha256: sha,
      remote_host: 'fake-host',
      remote_dir: '/srv/example-project'
    }
  },
  'remote-safety.json': {
    run_id: 'run-fixture',
    tab_id: 1,
    timestamp: '2026-05-31T12:08:25Z',
    kind: 'remote-safety',
    severity: 'info',
    message: 'upload verified',
    fields: { phase: 'upload-verified', remote_sha256: sha }
  },
  'deploy-finished.json': {
    run_id: 'run-fixture',
    tab_id: 1,
    timestamp: '2026-05-31T12:10:48Z',
    kind: 'deploy-finished',
    severity: 'info',
    message: 'deploy finished',
    fields: {
      outcome: 'succeeded',
      local_sha256: sha,
      remote_sha256: sha,
      post_head: 'abc1234deadbeef',
      receipt_path: '/artifacts/receipts/run-fixture/run-fixture-tab-01-deploy.json'
    }
  },
  'error.json': {
    run_id: 'run-fixture',
    tab_id: 1,
    timestamp: '2026-05-31T12:11:00Z',
    kind: 'error',
    severity: 'error',
    message: 'bridge protocol error',
    fields: { kind: 'protocol', recoverable: 'false' }
  }
};

function jsonText(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function expectedFiles() {
  const files = new Map();
  files.set(resolve(root, 'contracts/json-schema/event.schema.json'), jsonText(schema));
  for (const [name, value] of Object.entries(fixtures)) {
    files.set(resolve(root, 'contracts/fixtures/events', name), jsonText(value));
  }
  return files;
}

const mode = process.argv[2];
if (mode !== '--check' && mode !== '--write') {
  console.error('usage: scripts/generate-contracts.mjs [--check|--write]');
  process.exit(2);
}

const drift = [];
for (const [path, text] of expectedFiles()) {
  if (mode === '--write') {
    await mkdir(dirname(path), { recursive: true });
    await writeFile(path, text);
  } else if (!existsSync(path) || (await readFile(path, 'utf8')) !== text) {
    drift.push(relative(root, path));
  }
}

if (drift.length > 0) {
  console.error(`contract artifact drift: ${drift.join(', ')}`);
  process.exit(1);
}

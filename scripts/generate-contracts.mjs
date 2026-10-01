#!/usr/bin/env node
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { workflowFixtures } from './lib/workflow-fixtures.mjs';
import { compile } from 'json-schema-to-typescript';
import Ajv2020 from 'ajv/dist/2020.js';
import standaloneCode from 'ajv/dist/standalone/index.js';
import { dirname, resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const sha = 'ff297136810aa7374c842e77f57f506dd631be7033975ce6d454bdc82086ded4';

const schemas = JSON.parse(execFileSync('cargo', ['run', '--quiet', '--locked', '-p', 'jailgun-workflow', '--example', 'contract_schema'], {
  cwd: root, encoding: 'utf8', maxBuffer: 8 * 1024 * 1024,
  env: { ...process.env, CARGO_INCREMENTAL: process.env.CARGO_INCREMENTAL || '0', CARGO_PROFILE_DEV_DEBUG: process.env.CARGO_PROFILE_DEV_DEBUG || 'line-tables-only' },
}));
const metadata = (source) => ({ by: 'scripts/generate-contracts.mjs', note: 'DO NOT EDIT BY HAND', source, command: 'bash ops/ci/contracts.sh --write' });
const schema = { generated: metadata('crates/jailgun-core/src/event.rs'), ...schemas.event };
const workflowSchema = { generated: metadata('crates/jailgun-workflow/src/model.rs'), ...schemas.workflow };

const fixtures = {
  'run-queued.json': {
    run_id: 'run-fixture', tab_id: null, timestamp: '2026-05-31T11:59:57Z',
    kind: 'run-queued', severity: 'info', message: 'run queued', fields: { tabs: '5' }
  },
  'browser-lease-acquired.json': {
    run_id: 'run-fixture', tab_id: null, timestamp: '2026-05-31T11:59:58Z',
    kind: 'browser-lease-acquired', severity: 'info', message: 'browser capacity reserved', fields: { account_id: 'synthetic-account', tabs: '5' }
  },
  'browser-lease-released.json': {
    run_id: 'run-fixture', tab_id: null, timestamp: '2026-05-31T12:11:01Z',
    kind: 'browser-lease-released', severity: 'info', message: 'browser capacity released', fields: { account_id: 'synthetic-account', tabs: '5' }
  },
  'run-started.json': {
    run_id: 'run-fixture',
    tab_id: null,
    timestamp: '2026-05-31T12:00:00Z',
    kind: 'run-started',
    severity: 'info',
    message: 'run started',
    fields: { config: 'config/jailgun.example.toml', tabs: '5' }
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
  'auth-state.json': {
    run_id: 'auth-acct-fixture',
    tab_id: null,
    timestamp: '2026-05-31T12:06:50Z',
    kind: 'auth-state',
    severity: 'info',
    message: 'auth state updated',
    fields: {
      state: 'code-requested',
      page_url: 'https://chatgpt.com/',
      composer_detected: 'false',
      code_requested: 'true'
    }
  },
  'auth-action-needed.json': {
    run_id: 'auth-acct-fixture',
    tab_id: null,
    timestamp: '2026-05-31T12:06:51Z',
    kind: 'auth-action-needed',
    severity: 'warn',
    message: 'manual browser auth action needed',
    fields: {
      action: 'manual-browser-required',
      reason: 'password prompt detected'
    }
  },
  'auth-code-requested.json': {
    run_id: 'auth-acct-fixture',
    tab_id: null,
    timestamp: '2026-05-31T12:06:52Z',
    kind: 'auth-code-requested',
    severity: 'info',
    message: 'auth email code requested',
    fields: {
      channel: 'email',
      destination_hint: 'email verification'
    }
  },
  'auth-code-submitted.json': {
    run_id: 'auth-acct-fixture',
    tab_id: null,
    timestamp: '2026-05-31T12:06:53Z',
    kind: 'auth-code-submitted',
    severity: 'info',
    message: 'auth code submitted',
    fields: {
      accepted: 'true'
    }
  },
  'auth-complete.json': {
    run_id: 'auth-acct-fixture',
    tab_id: null,
    timestamp: '2026-05-31T12:06:54Z',
    kind: 'auth-complete',
    severity: 'info',
    message: 'auth complete',
    fields: {
      page_url: 'https://chatgpt.com/',
      composer_detected: 'true'
    }
  },
  'auth-failed.json': {
    run_id: 'auth-acct-fixture',
    tab_id: null,
    timestamp: '2026-05-31T12:06:55Z',
    kind: 'auth-failed',
    severity: 'error',
    message: 'auth failed',
    fields: {
      reason: 'manual browser required',
      manual_browser_required: 'true'
    }
  },
  'session-expired.json': {
    run_id: 'auth-acct-fixture',
    tab_id: null,
    timestamp: '2026-05-31T12:06:56Z',
    kind: 'session-expired',
    severity: 'warn',
    message: 'session expired',
    fields: {
      page_url: 'https://chatgpt.com/',
      reason: 'session expired prompt detected'
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
      local_path: 'downloads/source-fixes.tar.gz',
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

async function expectedFiles() {
  const files = new Map();
  files.set(resolve(root, 'contracts/json-schema/event.schema.json'), jsonText(schema));
  files.set(resolve(root, 'contracts/json-schema/workflow.schema.json'), jsonText(workflowSchema));
  files.set(resolve(root, 'apps/dashboard/src/generated/event.ts'), (await compile(schemas.event, 'JailgunEvent', { unreachableDefinitions: true, bannerComment: '/** Generated from Rust by scripts/generate-contracts.mjs. DO NOT EDIT. */' })) + `\nexport const EVENT_KINDS = ${JSON.stringify(schemas.event.$defs.EventKind.enum)} as const;\n`);
  files.set(resolve(root, 'apps/dashboard/src/generated/workflow.ts'), await compile(schemas.workflow, 'WorkflowContracts', { unreachableDefinitions: true, bannerComment: '/** Generated from Rust by scripts/generate-contracts.mjs. DO NOT EDIT. */', additionalProperties: false, ignoreMinAndMaxItems: true }));
  const banner = '/** Generated from Rust schemas by scripts/generate-contracts.mjs. DO NOT EDIT. */\n';
  const schemaId = 'https://jailgun.invalid/contracts/workflow';
  const validator = new Ajv2020({ code: { source: true, lines: true }, strict: true, inlineRefs: false, formats: { int64: true, uint16: true, uint32: true, uint64: true, int32: true, double: true, uint: true } });
  validator.addSchema({ ...schemas.workflow, $id: schemaId });
  const names = Object.fromEntries(Object.keys(schemas.workflow.properties).map(name => [name, `${schemaId}#/properties/${name}`]));
  files.set(resolve(root, 'apps/dashboard/src/generated/workflow-validators.cjs'), banner + standaloneCode(validator, names));
  files.set(resolve(root, 'apps/dashboard/src/generated/workflow-validators.d.cts'), banner + `import type { WorkflowContracts } from './workflow';\ndeclare const validators: { [K in keyof WorkflowContracts]: (value: unknown) => value is WorkflowContracts[K] };\nexport = validators;\n`);
  files.set(resolve(root, 'apps/dashboard/src/generated/workflow-defaults.ts'), banner + `import type { Criterion } from './workflow';\nexport const DEFAULT_CRITERIA: Criterion[] = ${JSON.stringify(schemas.workflow.$defs.ConceptRequest.properties.criteria.default, null, 2)};\n`);
  const workflowExamples = workflowFixtures(root);
  for (const [name, value] of Object.entries(workflowExamples)) {
    const kind = name.startsWith('account-') ? 'account_session' : name.startsWith('run-') || name.startsWith('after-') ? 'run' : null;
    if (kind && !validator.getSchema(names[kind])(value)) throw new Error(`Invalid workflow fixture ${name}`);
  }
  files.set(resolve(root, 'contracts/fixtures/workflow/states.json'), jsonText(workflowExamples));
  files.set(resolve(root, 'apps/dashboard/src/generated/workflow-fixtures.ts'), banner + `export const workflowFixtures = ${JSON.stringify(workflowExamples, null, 2)} as const;\n`);
  for (const [name, value] of Object.entries(fixtures)) {
    if (!validator.validate(schemas.event, value)) throw new Error(`Invalid event fixture ${name}`);
    files.set(resolve(root, 'contracts/fixtures/events', name), jsonText(value));
  }
  const eventImports = Object.keys(fixtures).map((name, index) => `import fixture${index} from '../../../../contracts/fixtures/events/${name}';`).join('\n');
  const eventExports = Object.keys(fixtures).map((name, index) => `  ${JSON.stringify(name)}: fixture${index},`).join('\n');
  files.set(resolve(root, 'apps/dashboard/src/generated/event-fixtures.ts'), banner + eventImports + `\n\nexport const eventFixtures = {\n${eventExports}\n};\n`);
  return files;
}

const mode = process.argv[2];
if (mode !== '--check' && mode !== '--write') {
  console.error('usage: scripts/generate-contracts.mjs [--check|--write]');
  process.exit(2);
}

const drift = [];
for (const [path, text] of await expectedFiles()) {
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

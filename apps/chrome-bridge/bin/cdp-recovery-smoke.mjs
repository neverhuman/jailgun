#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const PROTOCOL_VERSION = 1;
const DEFAULT_CDP_HOST = '127.0.0.1';
const DEFAULT_CDP_PORT = 9224;
const scriptDir = dirname(fileURLToPath(import.meta.url));
const appDir = join(scriptDir, '..');
const envPath = join(appDir, 'cdp-recovery-smoke.env');

let smokeEnv;
try {
  smokeEnv = await readEnvFile(envPath);
} catch (error) {
  printFailure({
    cause: `could not load smoke defaults from ${envPath}: ${error?.message || String(error)}`,
    checkedEndpoint: 'unavailable',
    checkedPort: 'unavailable',
    nextAction: `verify ${envPath} exists and is readable`,
  });
  process.exit(1);
}

const childEnv = { ...process.env, ...smokeEnv };
const requestedUrl = requestedCdpUrl(childEnv);
const requestedEndpoint = endpointInfo(requestedUrl);
const timeoutMs = numberFrom(childEnv.JAILGUN_CHROME_TIMEOUT_MS, 45000) + 15000;

let sawReady = false;
let sawRecovery = false;
let shutdownSent = false;
let stdoutBuffer = '';
let stderrBuffer = '';
let finished = false;
let resolvedUrl = '';
let checkedEndpoint = requestedEndpoint?.versionUrl ?? requestedUrl;
let checkedPort = requestedEndpoint?.port ?? 'unavailable';
let nextAction = nextActionForPort(checkedPort);
let shutdownTimer = null;
let deadline = null;

const child = spawn(process.execPath, [join(scriptDir, 'chrome-bridge.mjs')], {
  cwd: appDir,
  env: childEnv,
  stdio: ['pipe', 'pipe', 'pipe'],
});

deadline = setTimeout(() => {
  fail(`timed out after ${timeoutMs}ms waiting for bridge-ready`);
}, timeoutMs);

child.stderr.setEncoding('utf8');
child.stderr.on('data', (chunk) => {
  stderrBuffer += chunk;
});

child.stdout.setEncoding('utf8');
child.stdout.on('data', (chunk) => {
  stdoutBuffer += chunk;
  let newline = stdoutBuffer.indexOf('\n');
  while (newline >= 0) {
    const line = stdoutBuffer.slice(0, newline);
    stdoutBuffer = stdoutBuffer.slice(newline + 1);
    handleBridgeLine(line);
    newline = stdoutBuffer.indexOf('\n');
  }
});

child.once('error', (error) => fail(`could not start chrome bridge: ${error?.message || String(error)}`));
child.once('close', (code, signal) => {
  clearTimer(deadline);
  clearTimer(shutdownTimer);
  if (finished) {
    return;
  }
  if (!sawReady) {
    fail(`bridge exited before ready: code=${code ?? ''} signal=${signal ?? ''}`);
    return;
  }
  if (code !== 0 || signal) {
    fail(`bridge exited after ready with code=${code ?? ''} signal=${signal ?? ''}`);
    return;
  }
  succeed();
});

send('hello', {
  orchestrator_version: 'smoke',
  protocol_version: PROTOCOL_VERSION,
  capabilities: ['managed-chrome'],
});

function handleBridgeLine(line) {
  if (!line.trim()) {
    return;
  }
  let envelope;
  try {
    envelope = JSON.parse(line);
  } catch {
    return;
  }
  if (envelope.type === 'bridge-log' && envelope.payload?.phase === 'cdp-recovery') {
    sawRecovery = true;
    updateFromRecoveryFields(envelope.payload.fields ?? {});
  }
  if (envelope.type === 'bridge-ready') {
    sawReady = true;
    resolvedUrl = envelope.payload?.cdp_url || resolvedUrl || requestedUrl;
    requestShutdown();
  }
  if (envelope.type === 'error') {
    const message = envelope.payload?.message || 'bridge emitted startup error';
    updateFromFailureMessage(message);
    fail(message);
  }
}

function requestShutdown() {
  if (shutdownSent) {
    return;
  }
  shutdownSent = true;
  send('shutdown', { drain_timeout_ms: 1000 });
  child.stdin.end();
  shutdownTimer = setTimeout(() => {
    child.kill('SIGTERM');
  }, 5000);
}

function send(type, payload) {
  child.stdin.write(`${JSON.stringify({
    v: PROTOCOL_VERSION,
    type,
    run_id: 'cdp-recovery-smoke',
    ts: new Date().toISOString(),
    payload,
  })}\n`);
}

function succeed() {
  if (finished) {
    return;
  }
  finished = true;
  clearTimer(deadline);
  clearTimer(shutdownTimer);
  process.stdout.write([
    'SUCCESS: Chrome bridge CDP recovery verified',
    `requested URL: ${requestedUrl}`,
    `resolved URL: ${resolvedUrl || requestedUrl}`,
    `recovery observed: ${String(sawRecovery)}`,
  ].join('\n') + '\n');
}

function fail(message) {
  if (finished) {
    return;
  }
  finished = true;
  clearTimer(deadline);
  clearTimer(shutdownTimer);
  updateFromFailureMessage(message);
  const cause = firstFailureLine(message);
  const stderrHint = !cause && stderrBuffer.trim() ? firstFailureLine(stderrBuffer) : '';
  printFailure({
    cause: cause || stderrHint || 'unknown failure',
    checkedEndpoint,
    checkedPort,
    nextAction,
  });
  child.kill('SIGTERM');
  process.exitCode = 1;
}

function printFailure(details) {
  process.stderr.write([
    `FAILED: ${details.cause}`,
    `checked endpoint: ${details.checkedEndpoint}`,
    `checked port: ${details.checkedPort}`,
    `next action: ${details.nextAction}`,
  ].join('\n') + '\n');
}

function updateFromRecoveryFields(fields) {
  const checked = splitFieldList(fields.checked_cdp_urls);
  if (checked.length > 0) {
    setCheckedEndpoint(checked[checked.length - 1]);
  }
  if (fields.selected_cdp_url) {
    resolvedUrl = fields.selected_cdp_url;
  } else if (fields.fallback_cdp_url) {
    resolvedUrl = fields.fallback_cdp_url;
  }
}

function updateFromFailureMessage(message) {
  const text = String(message || '');
  const endpoint = text.match(/^Checked endpoint:\s*(.+)$/mi)?.[1]?.trim();
  const port = text.match(/^Checked port:\s*(\d+)$/mi)?.[1]?.trim();
  const action = text.match(/^Next action:\s*(.+)$/mi)?.[1]?.trim();
  if (endpoint) {
    checkedEndpoint = endpoint;
    const parsed = endpointInfo(endpoint);
    if (parsed?.port) {
      checkedPort = parsed.port;
    }
  }
  if (port) {
    checkedPort = port;
  }
  if (action) {
    nextAction = action;
  } else if (checkedPort !== 'unavailable') {
    nextAction = nextActionForPort(checkedPort);
  }
}

function setCheckedEndpoint(url) {
  const parsed = endpointInfo(url);
  if (!parsed) {
    checkedEndpoint = url;
    return;
  }
  checkedEndpoint = parsed.versionUrl;
  checkedPort = parsed.port;
  nextAction = nextActionForPort(parsed.port);
}

function requestedCdpUrl(env) {
  if (env.JAILGUN_CDP_URL) {
    return env.JAILGUN_CDP_URL;
  }
  const host = env.JAILGUN_CDP_HOST || env.GOOGLE_AUTOMATION_REMOTE_DEBUG_HOST || DEFAULT_CDP_HOST;
  const port = env.JAILGUN_CDP_PORT || env.GOOGLE_AUTOMATION_REMOTE_DEBUG_PORT || DEFAULT_CDP_PORT;
  return `http://${host}:${port}`;
}

function endpointInfo(url) {
  try {
    const parsed = new URL(url);
    const port = Number(parsed.port || (parsed.protocol === 'https:' ? 443 : 80));
    return {
      origin: parsed.origin,
      versionUrl: `${parsed.origin}/json/version`,
      port: Number.isFinite(port) ? String(port) : 'unavailable',
    };
  } catch {
    return null;
  }
}

async function readEnvFile(path) {
  const text = await readFile(path, 'utf8');
  const values = {};
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line || line.startsWith('#')) {
      continue;
    }
    const index = line.indexOf('=');
    if (index <= 0) {
      continue;
    }
    const key = line.slice(0, index).trim();
    let value = line.slice(index + 1).trim();
    if ((value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'"))) {
      value = value.slice(1, -1);
    }
    values[key] = value;
  }
  return values;
}

function splitFieldList(value) {
  return String(value || '')
    .split(',')
    .map((entry) => entry.trim())
    .filter(Boolean);
}

function numberFrom(value, defaultValue) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : defaultValue;
}

function nextActionForPort(port) {
  return port === 'unavailable'
    ? `inspect ${envPath} and JAILGUN_CDP_URL`
    : `lsof -nP -iTCP:${port} -sTCP:LISTEN`;
}

function firstFailureLine(value) {
  return String(value || '')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)[0] ?? '';
}

function clearTimer(timer) {
  if (timer) {
    clearTimeout(timer);
  }
}

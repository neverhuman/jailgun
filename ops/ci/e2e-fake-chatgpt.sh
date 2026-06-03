#!/usr/bin/env bash
# End-to-end CI lane: drive the full jailgun stack against the
# fake-chatgpt simulator. No real ChatGPT session burned; no SSH; no
# Telegram. Asserts looped, headless, artifact-scoped two-profile fan-out
# and verifies post-run cleanup.

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=ops/ci/lib.sh
source "$script_dir/lib.sh"
ci_enter_repo_root "$script_dir"
ci_require_cmd node
ci_require_cmd cargo
ci_require_cmd lsof

ARTIFACTS_DIR="${E2E_FAKE_CHATGPT_DIR:-target/e2e-fake-chatgpt}"
PORT="${FAKE_CHATGPT_PORT:-8082}"
CDP_PORT="${E2E_FAKE_CHATGPT_CDP_PORT:-9224}"
TABS="${E2E_FAKE_CHATGPT_TABS:-10}"
LOOPS="${E2E_FAKE_CHATGPT_LOOPS:-2}"

require_uint() {
  local name="${1:?name required}"
  local value="${2:?value required}"
  if [[ ! "$value" =~ ^[0-9]+$ ]]; then
    ci_log "$name must be an unsigned integer, got: $value"
    exit 1
  fi
}

require_uint "E2E_FAKE_CHATGPT_TABS" "$TABS"
require_uint "E2E_FAKE_CHATGPT_LOOPS" "$LOOPS"
require_uint "FAKE_CHATGPT_PORT" "$PORT"
require_uint "E2E_FAKE_CHATGPT_CDP_PORT" "$CDP_PORT"

if (( TABS <= 0 )); then
  ci_log "E2E_FAKE_CHATGPT_TABS must be positive"
  exit 1
fi
if (( TABS % 2 != 0 )); then
  ci_log "E2E_FAKE_CHATGPT_TABS must be even for two-profile proof, got: $TABS"
  exit 1
fi
if (( CDP_PORT <= 0 || CDP_PORT >= 65535 )); then
  ci_log "E2E_FAKE_CHATGPT_CDP_PORT must leave room for two CDP ports, got: $CDP_PORT"
  exit 1
fi
if (( PORT <= 0 || PORT > 65535 )); then
  ci_log "FAKE_CHATGPT_PORT is outside the TCP port range: $PORT"
  exit 1
fi

CDP_PORT_B=$((CDP_PORT + 1))
BATCH_COUNT=$((LOOPS + 1))
PLANNED_TABS=$((TABS * BATCH_COUNT))

PROFILE_POOL_DIRS=(
  "$ARTIFACTS_DIR/profiles/google-a"
  "$ARTIFACTS_DIR/profiles/google-b"
)
DOWNLOADS_DIR="$ARTIFACTS_DIR/downloads"
RECEIPTS_DIR="$ARTIFACTS_DIR/receipts"
EVENTS_LOG="$ARTIFACTS_DIR/events.ndjson"
CONFIG_PATH="$ARTIFACTS_DIR/jailgun.fake-chatgpt.toml"
PROMPT_PATH="$ARTIFACTS_DIR/prompt.txt"
RUN_ID="e2e-fake-chatgpt-$(date -u +%Y%m%dT%H%M%SZ)"
PROFILE_PROOF="$ARTIFACTS_DIR/profile-proof.json"

runner_pid=""
watchdog_pid=""
fake_pid=""
ARTIFACTS_DIR_ABS=""

listener_pids() {
  local port="${1:?port required}"
  lsof -nP -iTCP:"$port" -sTCP:LISTEN -t 2>/dev/null | sort -u || true
}

ensure_port_free() {
  local port="${1:?port required}"
  local label="${2:?label required}"
  local pids
  pids="$(listener_pids "$port")"
  if [[ -n "$pids" ]]; then
    ci_log "$label port $port is already listening; refusing to reuse a local browser/service"
    lsof -nP -iTCP:"$port" -sTCP:LISTEN >&2 || true
    exit 1
  fi
}

wait_for_no_listener() {
  local port="${1:?port required}"
  for _ in $(seq 1 50); do
    if [[ -z "$(listener_pids "$port")" ]]; then
      return 0
    fi
    sleep 0.1
  done
  return 1
}

kill_pid_gracefully() {
  local pid="${1:-}"
  if [[ -z "$pid" ]] || ! kill -0 "$pid" 2>/dev/null; then
    return 0
  fi
  kill "$pid" 2>/dev/null || true
  for _ in $(seq 1 20); do
    if ! kill -0 "$pid" 2>/dev/null; then
      return 0
    fi
    sleep 0.1
  done
  kill -KILL "$pid" 2>/dev/null || true
}

cleanup_artifact_pids() {
  local dir="${1:-}"
  [[ -n "$dir" && -d "$dir" ]] || return 0
  local pids
  pids="$(lsof -t +D "$dir" 2>/dev/null | sort -u || true)"
  [[ -n "$pids" ]] || return 0
  while read -r pid; do
    [[ -n "$pid" && "$pid" != "$$" ]] || continue
    kill "$pid" 2>/dev/null || true
  done <<<"$pids"
  sleep 1
  pids="$(lsof -t +D "$dir" 2>/dev/null | sort -u || true)"
  while read -r pid; do
    [[ -n "$pid" && "$pid" != "$$" ]] || continue
    kill -KILL "$pid" 2>/dev/null || true
  done <<<"$pids"
}

cleanup() {
  local status=$?
  set +e
  kill_pid_gracefully "$watchdog_pid"
  kill_pid_gracefully "$runner_pid"
  kill_pid_gracefully "$fake_pid"
  cleanup_artifact_pids "$ARTIFACTS_DIR_ABS"
  exit "$status"
}

trap cleanup EXIT INT TERM

rm -rf "$ARTIFACTS_DIR"
mkdir -p "$ARTIFACTS_DIR" "$DOWNLOADS_DIR" "$RECEIPTS_DIR" "${PROFILE_POOL_DIRS[@]}"
ARTIFACTS_DIR_ABS="$(cd "$ARTIFACTS_DIR" && pwd -P)"
PROFILE_POOL_DIRS_ABS=(
  "$ARTIFACTS_DIR_ABS/profiles/google-a"
  "$ARTIFACTS_DIR_ABS/profiles/google-b"
)
PROFILE_DIR="${PROFILE_POOL_DIRS_ABS[0]}"
DOWNLOADS_DIR_ABS="$ARTIFACTS_DIR_ABS/downloads"
STATE_DIR_ABS="$ARTIFACTS_DIR_ABS/chrome-state"

ensure_port_free "$PORT" "fake-chatgpt"
ensure_port_free "$CDP_PORT" "managed Chrome CDP slot 1"
ensure_port_free "$CDP_PORT_B" "managed Chrome CDP slot 2"

cat > "$CONFIG_PATH" <<EOF
[project]
name = "jailgun-e2e-fake-chatgpt"
repository = "git@example.com:org/jailgun.git"

[browser]
chat_url = "http://127.0.0.1:${PORT}/new"
model = "pro-extended"
tabs = ${TABS}
poll_interval_seconds = 1
completion_check_seconds = 1
submit_delay_seconds = 0
submit_jitter_seconds = 0
tar_wait_minutes = 1
profile_dir_env = "JAILGUN_CHROME_PROFILE_DIR"
state_dir_env = "JAILGUN_CHROME_STATE_DIR"

[paths]
artifacts_dir = "${ARTIFACTS_DIR_ABS}"
downloads_dir_env = "JAILGUN_DOWNLOADS_DIR"

[source_archive]
enabled = true
repo_url_env = "JAILGUN_SOURCE_REPO_URL"
ref_name = "HEAD"
prefix = "source/"
archive_filename = "source.tar.gz"
delete_after_upload = true

[deploy]
enabled = false
dry_run = true
remote_host_env = "JAILGUN_REMOTE_HOST"
remote_dir_env = "JAILGUN_REMOTE_DIR"
remote_command_env = "JAILGUN_REMOTE_COMMAND"
remote_strip_components = 1
remote_cleanup_policy = "preserve-reset"
remote_status_poll_seconds = 1
remote_job_delay_seconds = 0
remote_job_jitter_seconds = 0

[prompt_policy]
deny_github_write_by_default = true
allow_info_prompts = true
allowed_repositories = ["neverhuman/jailgun"]
EOF

printf 'E2E smoke prompt for fake-chatgpt. Ignored by the fake server.\n' > "$PROMPT_PATH"

ci_log "starting fake-chatgpt on port ${PORT}"
FAKE_CHATGPT_PORT="$PORT" node scripts/start-fake-chatgpt-with-cdp.mjs > "$ARTIFACTS_DIR/fake-chatgpt.log" 2>&1 &
fake_pid=$!

for _ in $(seq 1 60); do
  if grep -q '^READY ' "$ARTIFACTS_DIR/fake-chatgpt.log" 2>/dev/null; then
    break
  fi
  if ! kill -0 "$fake_pid" 2>/dev/null; then
    ci_log "fake-chatgpt exited before ready"
    cat "$ARTIFACTS_DIR/fake-chatgpt.log" >&2 || true
    exit 1
  fi
  sleep 0.5
done

if ! grep -q '^READY ' "$ARTIFACTS_DIR/fake-chatgpt.log"; then
  ci_log "fake-chatgpt did not become ready"
  cat "$ARTIFACTS_DIR/fake-chatgpt.log" >&2 || true
  exit 1
fi

ci_log "running headless fake-chatgpt smoke tabs=${TABS} loops=${LOOPS} planned_tabs=${PLANNED_TABS}"
set +e
JAILGUN_CHROME_PROFILE_DIR="$PROFILE_DIR" \
JAILGUN_CHROME_STATE_DIR="$STATE_DIR_ABS" \
JAILGUN_CHROME_HEADLESS=true \
JAILGUN_CDP_HOST="127.0.0.1" \
JAILGUN_CDP_PORT="$CDP_PORT" \
JAILGUN_DOWNLOADS_DIR="$DOWNLOADS_DIR_ABS" \
JAILGUN_ARTIFACTS_DIR="$ARTIFACTS_DIR_ABS" \
JAILGUN_SOURCE_REPO_URL="$(pwd)" \
JAILGUN_REMOTE_HOST="example.invalid" \
JAILGUN_REMOTE_DIR="/tmp/jailgun-e2e-fake" \
JAILGUN_REMOTE_COMMAND="true" \
cargo run --locked --quiet -p jailgun-cli --bin jailgun -- run \
  --config "$CONFIG_PATH" \
  --prompt-file "$PROMPT_PATH" \
  --run-id "$RUN_ID" \
  --tabs "$TABS" \
  --loops "$LOOPS" \
  --profile-pool "${PROFILE_POOL_DIRS_ABS[0]}" \
  --profile-pool "${PROFILE_POOL_DIRS_ABS[1]}" \
  --no-deploy \
  --artifacts-dir "$ARTIFACTS_DIR_ABS" \
  --downloads-dir "$DOWNLOADS_DIR_ABS" \
  --event-buffer 2048 \
  > "$ARTIFACTS_DIR/runner.log" 2> >(tee -a "$ARTIFACTS_DIR/runner.log" >&2) &
runner_pid=$!
runner_exit=
(sleep 240 && kill "$runner_pid" 2>/dev/null) &
watchdog_pid=$!

wait "$runner_pid"
runner_exit=$?
kill_pid_gracefully "$watchdog_pid"
watchdog_pid=""
runner_pid=""
set -e

ci_log "runner exited with status ${runner_exit}"

if [[ ! -s "$EVENTS_LOG" ]]; then
  EVENTS_LOG="$ARTIFACTS_DIR/runner.log"
fi

proof_exit=0
node - "$EVENTS_LOG" "$PROFILE_PROOF" "${PROFILE_POOL_DIRS_ABS[0]}" "${PROFILE_POOL_DIRS_ABS[1]}" "$TABS" "$LOOPS" <<'NODE' || proof_exit=$?
const fs = require('fs');

const [inputPath, proofPath, profileA, profileB, batchTabsRaw, loopsRaw] = process.argv.slice(2);
const batchTabs = Number(batchTabsRaw);
const loopCount = Number(loopsRaw);
const batchCount = loopCount + 1;
const plannedTabs = batchTabs * batchCount;
const expectedPerProfilePerBatch = batchTabs / 2;
const expectedTotalPerProfile = expectedPerProfilePerBatch * batchCount;
const expectedProfiles = [profileA, profileB];
const expectedSlots = ['1', '2'];
const failures = [];

function fail(message) {
  failures.push(message);
}

function increment(map, key) {
  map.set(key, (map.get(key) || 0) + 1);
}

function objectFromMap(map) {
  return Object.fromEntries([...map.entries()].sort(([left], [right]) => String(left).localeCompare(String(right))));
}

let events = [];
try {
  const raw = fs.readFileSync(inputPath, 'utf8');
  const lines = raw.split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
  for (const [index, line] of lines.entries()) {
    try {
      events.push(JSON.parse(line));
    } catch (error) {
      if (!inputPath.endsWith('runner.log')) {
        fail(`failed to parse JSON event line ${index + 1}: ${error.message}`);
      }
    }
  }
} catch (error) {
  fail(`failed to read event log ${inputPath}: ${error.message}`);
}

const tabOpens = events.filter((event) => event.kind === 'tab-opened');
const archiveUploads = events.filter((event) => event.kind === 'archive-uploaded');
const promptSubmissions = events.filter((event) => event.kind === 'prompt-submitted');
const bundledPromptInjections = events.filter((event) =>
  event.kind === 'browser-log' &&
  event.fields?.phase === 'prompt-injected-during-upload' &&
  event.fields?.status === 'ok');
const orderedOpens = tabOpens
  .slice()
  .sort((left, right) => Number(left.tab_id || 0) - Number(right.tab_id || 0));
const profileCounts = new Map();
const slotCounts = new Map();
const cdpUrls = new Set();
const profileToCdp = new Map();
const slotToTimestamps = new Map();
const perBatch = Array.from({ length: batchCount }, (_, index) => ({
  batch_index: index,
  tab_id_start: index * batchTabs + 1,
  tab_id_end: (index + 1) * batchTabs,
  profile_counts: new Map(),
  slot_counts: new Map(),
}));
const seenTabIds = new Set();

if (!Number.isInteger(batchTabs) || batchTabs <= 0 || batchTabs % 2 !== 0) {
  fail(`batch tab count must be a positive even integer, got ${batchTabsRaw}`);
}
if (!Number.isInteger(loopCount) || loopCount < 0) {
  fail(`loop count must be a non-negative integer, got ${loopsRaw}`);
}
if (tabOpens.length !== plannedTabs) {
  fail(`expected ${plannedTabs} tab-opened events, found ${tabOpens.length}`);
}
if (archiveUploads.length !== plannedTabs) {
  fail(`expected ${plannedTabs} archive-uploaded events, found ${archiveUploads.length}`);
}
if (promptSubmissions.length !== plannedTabs) {
  fail(`expected ${plannedTabs} prompt-submitted events, found ${promptSubmissions.length}`);
}
if (bundledPromptInjections.length !== plannedTabs) {
  fail(`expected ${plannedTabs} bundled prompt injection events, found ${bundledPromptInjections.length}`);
}

for (const event of orderedOpens) {
  const tabId = Number(event.tab_id);
  const profileDir = event.fields?.browser_profile_dir;
  const slot = event.fields?.browser_slot;
  const cdpUrl = event.fields?.cdp_url;
  if (!Number.isInteger(tabId) || tabId <= 0) {
    fail(`tab-opened event missing positive tab_id: ${JSON.stringify(event)}`);
    continue;
  }
  if (seenTabIds.has(tabId)) {
    fail(`duplicate tab-opened tab_id ${tabId}`);
  }
  seenTabIds.add(tabId);
  if (tabId > plannedTabs) {
    fail(`tab-opened tab_id ${tabId} exceeds planned tab count ${plannedTabs}`);
  }
  if (!profileDir) {
    fail(`tab-opened event ${tabId} missing browser_profile_dir`);
    continue;
  }
  if (!slot) {
    fail(`tab-opened event ${tabId} missing browser_slot`);
    continue;
  }
  if (!cdpUrl) {
    fail(`tab-opened event ${tabId} missing cdp_url`);
    continue;
  }
  if (!expectedProfiles.includes(profileDir)) {
    fail(`tab-opened event ${tabId} used unexpected profile dir: ${profileDir}`);
  }
  if (!expectedSlots.includes(slot)) {
    fail(`tab-opened event ${tabId} used unexpected browser slot: ${slot}`);
  }
  increment(profileCounts, profileDir);
  increment(slotCounts, slot);
  cdpUrls.add(cdpUrl);
  if (!profileToCdp.has(profileDir)) {
    profileToCdp.set(profileDir, new Set());
  }
  profileToCdp.get(profileDir).add(cdpUrl);
  const batch = perBatch[Math.floor((tabId - 1) / batchTabs)];
  if (batch) {
    increment(batch.profile_counts, profileDir);
    increment(batch.slot_counts, slot);
  }
  const timestamp = Date.parse(event.timestamp);
  if (Number.isNaN(timestamp)) {
    fail(`invalid timestamp on tab-opened event ${tabId}: ${event.timestamp}`);
  } else {
    if (!slotToTimestamps.has(slot)) {
      slotToTimestamps.set(slot, []);
    }
    slotToTimestamps.get(slot).push(timestamp);
  }
}

for (let tabId = 1; tabId <= plannedTabs; tabId += 1) {
  if (!seenTabIds.has(tabId)) {
    fail(`missing tab-opened event for planned tab_id ${tabId}`);
  }
}
for (const profile of expectedProfiles) {
  if ((profileCounts.get(profile) || 0) !== expectedTotalPerProfile) {
    fail(`expected ${expectedTotalPerProfile} tab-opened events for ${profile}, found ${profileCounts.get(profile) || 0}`);
  }
  if ((profileToCdp.get(profile)?.size || 0) !== 1) {
    fail(`expected one cdp_url for ${profile}, found ${(profileToCdp.get(profile) || new Set()).size}`);
  }
}
for (const slot of expectedSlots) {
  if ((slotCounts.get(slot) || 0) !== expectedTotalPerProfile) {
    fail(`expected ${expectedTotalPerProfile} tab-opened events for browser slot ${slot}, found ${slotCounts.get(slot) || 0}`);
  }
}
if (profileCounts.size !== 2) {
  fail(`expected exactly 2 browser_profile_dir values, found ${profileCounts.size}`);
}
if (slotCounts.size !== 2) {
  fail(`expected exactly 2 browser_slot values, found ${slotCounts.size}`);
}
if (cdpUrls.size !== 2) {
  fail(`expected exactly 2 cdp_url values, found ${cdpUrls.size}`);
}
for (const batch of perBatch) {
  for (const profile of expectedProfiles) {
    if ((batch.profile_counts.get(profile) || 0) !== expectedPerProfilePerBatch) {
      fail(`batch ${batch.batch_index} expected ${expectedPerProfilePerBatch} opens for ${profile}, found ${batch.profile_counts.get(profile) || 0}`);
    }
  }
  for (const slot of expectedSlots) {
    if ((batch.slot_counts.get(slot) || 0) !== expectedPerProfilePerBatch) {
      fail(`batch ${batch.batch_index} expected ${expectedPerProfilePerBatch} opens for browser slot ${slot}, found ${batch.slot_counts.get(slot) || 0}`);
    }
  }
}

let overlappingWindows = null;
const [slotOneTimes, slotTwoTimes] = expectedSlots.map((slot) => slotToTimestamps.get(slot) || []);
if (slotOneTimes.length > 0 && slotTwoTimes.length > 0) {
  const slotOneWindow = [Math.min(...slotOneTimes), Math.max(...slotOneTimes)];
  const slotTwoWindow = [Math.min(...slotTwoTimes), Math.max(...slotTwoTimes)];
  const windowsOverlap = slotOneWindow[0] <= slotTwoWindow[1] && slotTwoWindow[0] <= slotOneWindow[1];
  if (!windowsOverlap) {
    fail(
      `expected overlapping tab-opened timestamps, got slot 1 [${new Date(slotOneWindow[0]).toISOString()}, ${new Date(slotOneWindow[1]).toISOString()}] and slot 2 [${new Date(slotTwoWindow[0]).toISOString()}, ${new Date(slotTwoWindow[1]).toISOString()}]`,
    );
  }
  overlappingWindows = {
    'browser-slot-1': {
      first_opened_at: new Date(slotOneWindow[0]).toISOString(),
      last_opened_at: new Date(slotOneWindow[1]).toISOString(),
    },
    'browser-slot-2': {
      first_opened_at: new Date(slotTwoWindow[0]).toISOString(),
      last_opened_at: new Date(slotTwoWindow[1]).toISOString(),
    },
    overlaps: windowsOverlap,
  };
}

const proof = {
  status: failures.length === 0 ? 'ok' : 'failed',
  batch_tabs: batchTabs,
  loop_count: loopCount,
  batch_count: batchCount,
  planned_tabs: plannedTabs,
  total_tab_opened_events: tabOpens.length,
  total_archive_uploaded_events: archiveUploads.length,
  total_prompt_submitted_events: promptSubmissions.length,
  total_bundled_prompt_injection_events: bundledPromptInjections.length,
  expected_per_profile_per_batch: expectedPerProfilePerBatch,
  expected_total_per_profile: expectedTotalPerProfile,
  profile_counts: objectFromMap(profileCounts),
  slot_counts: objectFromMap(slotCounts),
  cdp_urls: Array.from(cdpUrls).sort(),
  batches: perBatch.map((batch) => ({
    batch_index: batch.batch_index,
    tab_id_start: batch.tab_id_start,
    tab_id_end: batch.tab_id_end,
    profile_counts: objectFromMap(batch.profile_counts),
    slot_counts: objectFromMap(batch.slot_counts),
  })),
  overlapping_windows: overlappingWindows,
  tab_open_sequence: orderedOpens.map((event) => ({
    tab_id: event.tab_id,
    timestamp: event.timestamp,
    browser_profile_dir: event.fields?.browser_profile_dir || '',
    browser_slot: event.fields?.browser_slot || '',
    cdp_url: event.fields?.cdp_url || '',
  })),
  validation_failures: failures,
  cleanup: {
    status: 'not-run',
  },
};

fs.writeFileSync(proofPath, `${JSON.stringify(proof, null, 2)}\n`);
if (failures.length > 0) {
  for (const failure of failures) {
    console.error(`[ci] ${failure}`);
  }
  process.exit(1);
}
console.log(
  `[ci] proof ok: planned_tabs=${plannedTabs}, batches=${batchCount}, ${expectedPerProfilePerBatch} tabs/profile/batch, wrote ${proofPath}`,
);
NODE

kill_pid_gracefully "$fake_pid"
fake_pid=""
wait_for_no_listener "$PORT" || true
wait_for_no_listener "$CDP_PORT" || true
wait_for_no_listener "$CDP_PORT_B" || true

cleanup_exit=0
node - "$PROFILE_PROOF" "$ARTIFACTS_DIR_ABS" "$PORT" "$CDP_PORT" "$CDP_PORT_B" <<'NODE' || cleanup_exit=$?
const fs = require('fs');
const { spawnSync } = require('child_process');

const [proofPath, artifactDir, fakePortRaw, ...cdpPortRaw] = process.argv.slice(2);
const fakePort = Number(fakePortRaw);
const configuredCdpPorts = cdpPortRaw.map(Number);
let proof = {};
try {
  proof = JSON.parse(fs.readFileSync(proofPath, 'utf8'));
} catch (error) {
  proof = {
    status: 'failed',
    validation_failures: [`profile proof was missing before cleanup: ${error.message}`],
    cdp_urls: [],
  };
}

function unique(values) {
  return [...new Set(values.filter(Boolean))].sort((left, right) => Number(left) - Number(right));
}

function lsof(args) {
  const result = spawnSync('lsof', args, { encoding: 'utf8' });
  if (result.error) {
    return {
      ok: false,
      pids: [],
      status: null,
      stderr: result.error.message,
      command: `lsof ${args.join(' ')}`,
    };
  }
  return {
    ok: true,
    pids: unique((result.stdout || '').split(/\s+/).map((value) => value.trim())),
    status: result.status,
    stderr: (result.stderr || '').trim(),
    command: `lsof ${args.join(' ')}`,
  };
}

function listenerCheck(port, label) {
  const result = lsof(['-nP', `-iTCP:${port}`, '-sTCP:LISTEN', '-t']);
  return {
    label,
    port,
    ok: result.ok && result.pids.length === 0,
    pids: result.pids,
    command: result.command,
    status: result.status,
    stderr: result.stderr,
  };
}

const artifactResult = lsof(['-t', '+D', artifactDir]);
const artifactCheck = {
  label: 'artifact-open-files',
  artifact_dir: artifactDir,
  ok: artifactResult.ok && artifactResult.pids.length === 0,
  pids: artifactResult.pids,
  command: artifactResult.command,
  status: artifactResult.status,
  stderr: artifactResult.stderr,
};

const cdpPortsFromProof = [];
for (const cdpUrl of proof.cdp_urls || []) {
  try {
    cdpPortsFromProof.push(Number(new URL(cdpUrl).port));
  } catch {
    // The validation step reports malformed URLs separately.
  }
}
const cdpPorts = unique([...configuredCdpPorts, ...cdpPortsFromProof].map(String)).map(Number);
const fakeCheck = listenerCheck(fakePort, 'fake-chatgpt');
const cdpChecks = cdpPorts.map((port) => listenerCheck(port, 'managed-cdp'));
const checks = [artifactCheck, fakeCheck, ...cdpChecks];
const failures = checks
  .filter((check) => !check.ok)
  .map((check) => `${check.label}${check.port ? `:${check.port}` : ''} still live pids=${check.pids.join(',') || '(none)'}`);

proof.cleanup = {
  status: failures.length === 0 ? 'ok' : 'failed',
  checked_at: new Date().toISOString(),
  fake_chatgpt_port: fakePort,
  managed_cdp_ports: cdpPorts,
  artifact_dir: artifactDir,
  checks: {
    artifact_open_files: artifactCheck,
    fake_chatgpt_listener: fakeCheck,
    managed_cdp_listeners: cdpChecks,
  },
  failures,
};
if (proof.status === 'ok' && failures.length > 0) {
  proof.status = 'failed';
}

fs.writeFileSync(proofPath, `${JSON.stringify(proof, null, 2)}\n`);
if (failures.length > 0) {
  for (const failure of failures) {
    console.error(`[ci] cleanup failed: ${failure}`);
  }
  process.exit(1);
}
console.log(`[ci] cleanup ok: fake_port=${fakePort} cdp_ports=${cdpPorts.join(',')} artifact_pids=none`);
NODE

if [[ "$runner_exit" -ne 0 ]]; then
  ci_log "runner failed with status ${runner_exit}"
  tail -80 "$ARTIFACTS_DIR/runner.log" >&2 || true
  exit "$runner_exit"
fi
if [[ "$proof_exit" -ne 0 ]]; then
  ci_log "profile proof failed; see $PROFILE_PROOF"
  exit "$proof_exit"
fi
if [[ "$cleanup_exit" -ne 0 ]]; then
  ci_log "cleanup proof failed; see $PROFILE_PROOF"
  exit "$cleanup_exit"
fi

trap - EXIT INT TERM
ci_log "e2e-fake-chatgpt lane green (events: $(wc -l < "$EVENTS_LOG" | tr -d ' ') lines, proof: $PROFILE_PROOF)"

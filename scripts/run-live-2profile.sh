#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
cd "$repo_root"

env_file="${JAILGUN_LIVE_ENV_FILE:-$repo_root/.env.live-7tab}"
if [[ ! -f "$env_file" ]]; then
  printf 'FAILED: missing live runner config\nchecked path: %s\nnext action: create %s from .env.live-7tab.example\n' "$env_file" "$env_file" >&2
  exit 1
fi

set -a
# shellcheck source=/dev/null
source "$env_file"
set +a

log() {
  printf '[live-2profile] %s\n' "$*"
}

current_run_id="${JAILGUN_RUN_ID:-live-2profile-pending}"
dashboard_url="http://${JAILGUN_DASHBOARD_ADDR:-127.0.0.1:8787}"
runner_log=""
monitor_log=""
events_log=""
receipt_file=""
runner_pid=""
monitor_pid=""
proof_summary=""
cleanup_summary=""
cleanup_proof_file=""

fail() {
  local cause="${1:?cause required}"
  local checked="${2:-n/a}"
  local next_action="${3:-inspect the live run logs}"
  if [[ -n "$receipt_file" ]]; then
    {
      printf 'FAILED: %s\n' "$cause"
      printf 'run id: %s\n' "$current_run_id"
      printf 'dashboard URL: %s\n' "$dashboard_url"
      printf 'checked: %s\n' "$checked"
      printf 'next action: %s\n' "$next_action"
      printf 'logs: %s %s\n' "${runner_log:-n/a}" "${monitor_log:-n/a}"
    } | tee "$receipt_file" >&2
  else
    {
      printf 'FAILED: %s\n' "$cause"
      printf 'checked: %s\n' "$checked"
      printf 'next action: %s\n' "$next_action"
    } >&2
  fi
  exit 1
}

stage_mode="${JAILGUN_STAGE:-canary}"
preflight_only=0
JAILGUN_ENABLE_DEPLOY_STAGE="${JAILGUN_ENABLE_DEPLOY_STAGE:-0}"
JAILGUN_CANARY_TABS="${JAILGUN_CANARY_TABS:-2}"
JAILGUN_CANARY_LOOPS="${JAILGUN_CANARY_LOOPS:-0}"
JAILGUN_FULL_TABS="${JAILGUN_FULL_TABS:-10}"
JAILGUN_FULL_LOOPS="${JAILGUN_FULL_LOOPS:-2}"
JAILGUN_DEPLOY_TABS="${JAILGUN_DEPLOY_TABS:-${JAILGUN_TABS:-10}}"
JAILGUN_DEPLOY_LOOPS="${JAILGUN_DEPLOY_LOOPS:-${JAILGUN_LOOPS:-2}}"
JAILGUN_DEPLOY_CONCURRENCY="${JAILGUN_DEPLOY_CONCURRENCY:-1}"
JAILGUN_INITIAL_TAB_BURST="${JAILGUN_INITIAL_TAB_BURST:-}"
JAILGUN_CDP_PORT_START="${JAILGUN_CDP_PORT_START:-9224}"
JAILGUN_CDP_HOST="127.0.0.1"
JAILGUN_PROFILE_A_DIR="${JAILGUN_PROFILE_A_DIR:-$HOME/.jailgun/profiles/prod-google-a}"
JAILGUN_PROFILE_B_DIR="${JAILGUN_PROFILE_B_DIR:-$HOME/.jailgun/profiles/prod-google-b}"
JAILGUN_DASHBOARD_ADDR="${JAILGUN_DASHBOARD_ADDR:-127.0.0.1:8787}"
JAILGUN_DASHBOARD_HOLD_SECONDS="${JAILGUN_DASHBOARD_HOLD_SECONDS:-30}"
JAILGUN_DASHBOARD_START_TIMEOUT_SECONDS="${JAILGUN_DASHBOARD_START_TIMEOUT_SECONDS:-300}"
JAILGUN_DASHBOARD_KEEP_ALIVE="${JAILGUN_DASHBOARD_KEEP_ALIVE:-0}"
JAILGUN_CARGO_JOBS="${JAILGUN_CARGO_JOBS:-10}"
JAILGUN_FRESH_SOURCE_CLONE="${JAILGUN_FRESH_SOURCE_CLONE:-0}"
JAILGUN_SUBMIT_DELAY_SECONDS="${JAILGUN_SUBMIT_DELAY_SECONDS:-}"
JAILGUN_SUBMIT_JITTER_SECONDS="${JAILGUN_SUBMIT_JITTER_SECONDS:-}"
JAILGUN_SUBMIT_JITTER_PERCENT="${JAILGUN_SUBMIT_JITTER_PERCENT:-}"
JAILGUN_MAX_DOWNLOAD_START_LATENCY_MS="${JAILGUN_MAX_DOWNLOAD_START_LATENCY_MS:-10000}"
JAILGUN_MESSAGE_STREAM_RETRY_LIMIT="${JAILGUN_MESSAGE_STREAM_RETRY_LIMIT:-6}"
JAILGUN_MESSAGE_STREAM_RETRY_DELAY_MS="${JAILGUN_MESSAGE_STREAM_RETRY_DELAY_MS:-10000}"
JAILGUN_JMCP_INBOX_DIR="${JAILGUN_JMCP_INBOX_DIR:-$HOME/code/jmcp/inbox}"
JAILGUN_JMCP_INBOX_DIR_RESOLVED=""
JAILGUN_CI="${JAILGUN_CI:-0}"
JAILGUN_CI_BRANCH="${JAILGUN_CI_BRANCH:-main}"
JAILGUN_CI_MAX_ATTEMPTS="${JAILGUN_CI_MAX_ATTEMPTS:-60}"
JAILGUN_CI_POLL_SECONDS="${JAILGUN_CI_POLL_SECONDS:-20}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --stage)
      stage_mode="${2:-}"
      shift 2
      ;;
    --enable-deploy-stage)
      JAILGUN_ENABLE_DEPLOY_STAGE=1
      shift
      ;;
    --preflight-only)
      preflight_only=1
      shift
      ;;
    --canary-tabs)
      JAILGUN_CANARY_TABS="${2:-}"
      shift 2
      ;;
    --canary-loops)
      JAILGUN_CANARY_LOOPS="${2:-}"
      shift 2
      ;;
    --full-tabs)
      JAILGUN_FULL_TABS="${2:-}"
      shift 2
      ;;
    --full-loops)
      JAILGUN_FULL_LOOPS="${2:-}"
      shift 2
      ;;
    --deploy-tabs)
      JAILGUN_DEPLOY_TABS="${2:-}"
      shift 2
      ;;
    --deploy-loops)
      JAILGUN_DEPLOY_LOOPS="${2:-}"
      shift 2
      ;;
    --deploy-concurrency)
      JAILGUN_DEPLOY_CONCURRENCY="${2:-}"
      shift 2
      ;;
    --submit-delay-seconds)
      JAILGUN_SUBMIT_DELAY_SECONDS="${2:-}"
      shift 2
      ;;
    --submit-jitter-seconds)
      JAILGUN_SUBMIT_JITTER_SECONDS="${2:-}"
      shift 2
      ;;
    --submit-jitter-percent)
      JAILGUN_SUBMIT_JITTER_PERCENT="${2:-}"
      shift 2
      ;;
    --initial-tab-burst)
      JAILGUN_INITIAL_TAB_BURST="${2:-}"
      shift 2
      ;;
    *)
      fail "unknown argument $1" "$*" "use --stage canary|full|deploy|all, --enable-deploy-stage, --preflight-only, stage shape overrides, submit delay overrides, --deploy-concurrency, or --initial-tab-burst"
      ;;
  esac
done

case "$stage_mode" in
  canary|full|deploy|all) ;;
  *) fail "unknown stage $stage_mode" "$stage_mode" "use --stage canary, full, deploy, or all" ;;
esac

if [[ "$stage_mode" == "deploy" && "$JAILGUN_ENABLE_DEPLOY_STAGE" != "1" ]]; then
  fail "deploy stage requires an explicit gate" "--stage deploy" "rerun with --enable-deploy-stage after canary/full proof is green"
fi
if [[ "$JAILGUN_DASHBOARD_KEEP_ALIVE" != "0" ]]; then
  fail "dashboard keep-alive is not supported by the staged two-profile wrapper" "JAILGUN_DASHBOARD_KEEP_ALIVE=$JAILGUN_DASHBOARD_KEEP_ALIVE" "leave keep-alive disabled so cleanup can be proven"
fi

RUN_ID_PREFIX="${JAILGUN_RUN_ID:-live-2profile-$(date -u +%Y%m%dT%H%M%SZ)}"
export CARGO_TARGET_DIR="$repo_root/artifacts/live-runs/$RUN_ID_PREFIX/cargo-target"
export CARGO_NET_OFFLINE=1

cleanup() {
  local status=$?
  if [[ $status -ne 0 ]]; then
    if [[ -n "$monitor_pid" ]] && kill -0 "$monitor_pid" 2>/dev/null; then
      kill "$monitor_pid" 2>/dev/null || true
    fi
    if [[ -n "$runner_pid" ]] && kill -0 "$runner_pid" 2>/dev/null; then
      kill "$runner_pid" 2>/dev/null || true
    fi
  fi
}
trap cleanup EXIT

is_non_negative_int() {
  [[ "${1:-}" =~ ^[0-9]+$ ]]
}

is_positive_int() {
  [[ "${1:-}" =~ ^[0-9]+$ && "$1" -gt 0 ]]
}

expand_path() {
  local path="${1:?path required}"
  case "$path" in
    "~") printf '%s\n' "$HOME" ;;
    "~/"*) printf '%s/%s\n' "$HOME" "${path#~/}" ;;
    "\$HOME") printf '%s\n' "$HOME" ;;
    "\$HOME/"*) printf '%s/%s\n' "$HOME" "${path#\$HOME/}" ;;
    *) printf '%s\n' "$path" ;;
  esac
}

require_profile_dir() {
  local value="${1:?profile dir required}"
  local label="${2:?profile label required}"
  local expanded
  expanded="$(expand_path "$value")"
  if [[ ! -d "$expanded" ]]; then
    fail "missing Chrome profile directory $label" "$expanded" "create it, log that profile into ChatGPT in headed Chrome, then rerun"
  fi
  if [[ ! -s "$expanded/Default/Preferences" ]]; then
    fail "profile metadata is missing for $label" "$expanded/Default/Preferences" "complete the manual headed login and close Chrome before rerunning"
  fi
  (cd "$expanded" && pwd -P)
}

require_env() {
  local name="${1:?env name required}"
  local value="${!name:-}"
  if [[ -z "$value" ]]; then
    fail "missing required environment value $name" "$env_file" "set $name in $env_file"
  fi
}

require_file() {
  local path="${1:?path required}"
  local label="${2:?label required}"
  if [[ ! -s "$path" ]]; then
    fail "$label is missing or empty" "$path" "restore $path before running production"
  fi
}

planned_tabs_for() {
  local tabs="${1:?tabs required}"
  local loops="${2:?loops required}"
  printf '%s\n' "$((tabs * (loops + 1)))"
}

port_is_listening() {
  local host="${1:?host required}"
  local port="${2:?port required}"
  if command -v lsof >/dev/null 2>&1 && lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1; then
    return 0
  fi
  if command -v nc >/dev/null 2>&1 && nc -z "$host" "$port" >/dev/null 2>&1; then
    return 0
  fi
  return 1
}

require_port_free() {
  local host="${1:?host required}"
  local port="${2:?port required}"
  local label="${3:?label required}"
  if port_is_listening "$host" "$port"; then
    fail "$label port is occupied" "$host:$port" "stop the listener before launching managed Chrome"
  fi
}

reject_forbidden_value() {
  local label="${1:?label required}"
  local value="${2:-}"
  case "$value" in
    *fake-chatgpt*|*127.0.0.1:8082*|*/new*)
      fail "forbidden production browser target detected" "$label=$value" "use real https://chatgpt.com/ and remove fake-chatgpt, 127.0.0.1:8082, or /new targets"
      ;;
  esac
}

config_chat_url() {
  awk '
    /^\[browser\]/ { in_browser = 1; next }
    /^\[/ { in_browser = 0 }
    in_browser && /^[[:space:]]*chat_url[[:space:]]*=/ {
      value = $0
      sub(/^[^=]*=/, "", value)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", value)
      gsub(/^"|"$/, "", value)
      print value
      exit
    }
  ' config/jailgun.example.toml
}

reject_forbidden_live_targets() {
  local chat_url
  chat_url="$(config_chat_url)"
  reject_forbidden_value "config/jailgun.example.toml browser.chat_url" "$chat_url"
  reject_forbidden_value "JAILGUN_CDP_URL" "${JAILGUN_CDP_URL:-}"
  reject_forbidden_value "JAILGUN_CHAT_URL" "${JAILGUN_CHAT_URL:-}"
  reject_forbidden_value "JAILGUN_BROWSER_CHAT_URL" "${JAILGUN_BROWSER_CHAT_URL:-}"
  while IFS= read -r line; do
    line="${line#"${line%%[![:space:]]*}"}"
    [[ -z "$line" || "${line:0:1}" == "#" ]] && continue
    reject_forbidden_value "$env_file" "$line"
  done < "$env_file"
}

validate_jmcp_inbox() {
  if [[ "${JAILGUN_NOTIFY_JMCP:-}" != "1" ]]; then
    fail "JMCP notifications are not enabled" "JAILGUN_NOTIFY_JMCP=${JAILGUN_NOTIFY_JMCP:-unset}" "set JAILGUN_NOTIFY_JMCP=1 in $env_file"
  fi
  local inbox_dir="${JAILGUN_JMCP_INBOX_DIR:-$HOME/code/jmcp/inbox}"
  inbox_dir="$(expand_path "$inbox_dir")"
  local parent_dir
  parent_dir="$(dirname "$inbox_dir")"
  if [[ ! -d "$parent_dir" ]]; then
    fail "JMCP inbox parent does not exist" "$parent_dir" "create $parent_dir or set JAILGUN_JMCP_INBOX_DIR to an existing JMCP repo"
  fi
  mkdir -p "$inbox_dir"
  chmod 700 "$inbox_dir" 2>/dev/null || true
  JAILGUN_JMCP_INBOX_DIR_RESOLVED="$inbox_dir"
}

validate_source_ref() {
  if [[ ! -d "$JAILGUN_SOURCE_REPO_URL/.git" ]]; then
    fail "source repository is not a git checkout" "$JAILGUN_SOURCE_REPO_URL" "set JAILGUN_SOURCE_REPO_URL to the local source checkout"
  fi
  git -C "$JAILGUN_SOURCE_REPO_URL" rev-parse --verify "$JAILGUN_SOURCE_REF^{commit}" >/dev/null ||
    fail "source ref is missing" "$JAILGUN_SOURCE_REPO_URL $JAILGUN_SOURCE_REF" "fetch the source repo or update JAILGUN_SOURCE_REF"
}

validate_remote() {
  local remote_dir_q
  remote_dir_q="$(printf '%q' "$JAILGUN_REMOTE_DIR")"
  local output
  local status
  set +e
  output="$(
    ssh -o BatchMode=yes -o ConnectTimeout=15 "$JAILGUN_REMOTE_HOST" "
      set -e
      test -d $remote_dir_q
      cd $remote_dir_q
      command -v bash >/dev/null
      git fetch origin main >/dev/null
      git rev-parse --verify origin/main >/dev/null
      remote_status=\$(git status --porcelain)
      if [ -n \"\$remote_status\" ]; then
        printf '%s\n' \"\$remote_status\" >&2
        exit 42
      fi
    " 2>&1
  )"
  status=$?
  set -e
  if [[ $status -eq 42 ]]; then
    fail "remote checkout is dirty before production" "$JAILGUN_REMOTE_HOST:$JAILGUN_REMOTE_DIR" "preserve or reset remote changes, then rerun; dirty files: $output"
  fi
  if [[ $status -ne 0 ]]; then
    fail "SSH remote checkout is not ready" "$JAILGUN_REMOTE_HOST:$JAILGUN_REMOTE_DIR" "verify SSH access, origin/main, git, bash, and remote dir; ssh output: $output"
  fi
}

validate_ci() {
  if [[ "$JAILGUN_CI" == "1" ]]; then
    require_env JAILGUN_CI_REPO
  fi
}

deploy_stage_requested() {
  [[ "$stage_mode" == "deploy" || ( "$stage_mode" == "all" && "$JAILGUN_ENABLE_DEPLOY_STAGE" == "1" ) ]]
}

validate_static_preflight() {
  for value in \
    "$JAILGUN_CANARY_TABS" "$JAILGUN_CANARY_LOOPS" \
    "$JAILGUN_FULL_TABS" "$JAILGUN_FULL_LOOPS" \
    "$JAILGUN_DEPLOY_TABS" "$JAILGUN_DEPLOY_LOOPS"; do
    if ! is_non_negative_int "$value"; then
      fail "stage tab and loop values must be non-negative integers" "$value" "fix stage shape values in $env_file or CLI args"
    fi
  done
  for value in "$JAILGUN_CANARY_TABS" "$JAILGUN_FULL_TABS" "$JAILGUN_DEPLOY_TABS"; do
    if [[ "$value" -le 0 ]]; then
      fail "stage tab counts must be positive" "$value" "use at least one tab per stage"
    fi
  done
  if [[ -n "$JAILGUN_SUBMIT_DELAY_SECONDS" ]] && ! is_non_negative_int "$JAILGUN_SUBMIT_DELAY_SECONDS"; then
    fail "submit delay must be a non-negative integer" "$JAILGUN_SUBMIT_DELAY_SECONDS" "pass --submit-delay-seconds 120"
  fi
  if [[ -n "$JAILGUN_INITIAL_TAB_BURST" ]] && ! is_non_negative_int "$JAILGUN_INITIAL_TAB_BURST"; then
    fail "initial tab burst must be a non-negative integer" "$JAILGUN_INITIAL_TAB_BURST" "pass --initial-tab-burst 10"
  fi
  if ! is_positive_int "$JAILGUN_DEPLOY_CONCURRENCY"; then
    fail "deploy concurrency must be a positive integer" "$JAILGUN_DEPLOY_CONCURRENCY" "pass --deploy-concurrency 1"
  fi
  if [[ -n "$JAILGUN_SUBMIT_JITTER_SECONDS" ]] && ! is_non_negative_int "$JAILGUN_SUBMIT_JITTER_SECONDS"; then
    fail "submit jitter must be a non-negative integer" "$JAILGUN_SUBMIT_JITTER_SECONDS" "pass --submit-jitter-seconds 0"
  fi
  if [[ -n "$JAILGUN_SUBMIT_JITTER_PERCENT" ]]; then
    if ! is_non_negative_int "$JAILGUN_SUBMIT_JITTER_PERCENT"; then
      fail "submit jitter percent must be a non-negative integer" "$JAILGUN_SUBMIT_JITTER_PERCENT" "pass --submit-jitter-percent 20"
    fi
    if [[ "$JAILGUN_SUBMIT_JITTER_PERCENT" -gt 100 ]]; then
      fail "submit jitter percent must be at most 100" "$JAILGUN_SUBMIT_JITTER_PERCENT" "pass --submit-jitter-percent 20"
    fi
  fi
  if ! is_positive_int "$JAILGUN_CDP_PORT_START"; then
    fail "CDP port start must be a positive integer" "$JAILGUN_CDP_PORT_START" "set JAILGUN_CDP_PORT_START=9224"
  fi

  profile_a_dir="$(require_profile_dir "$JAILGUN_PROFILE_A_DIR" JAILGUN_PROFILE_A_DIR)"
  profile_b_dir="$(require_profile_dir "$JAILGUN_PROFILE_B_DIR" JAILGUN_PROFILE_B_DIR)"
  if [[ "$profile_a_dir" == "$profile_b_dir" ]]; then
    fail "profile dirs must be distinct" "$profile_a_dir" "set JAILGUN_PROFILE_A_DIR and JAILGUN_PROFILE_B_DIR to two different headed-login profiles"
  fi

  require_port_free "$JAILGUN_CDP_HOST" "$JAILGUN_CDP_PORT_START" "CDP profile A"
  require_port_free "$JAILGUN_CDP_HOST" "$((JAILGUN_CDP_PORT_START + 1))" "CDP profile B"
  reject_forbidden_live_targets

  require_env JAILGUN_PROMPT_FILE
  require_env JAILGUN_SOURCE_REPO_URL
  require_env JAILGUN_SOURCE_REF
  require_env JAILGUN_EXPECTED_TOP_LEVEL
  require_env JAILGUN_TAR_TARGET_NAME
  require_file "$JAILGUN_PROMPT_FILE" "prompt file"
  validate_jmcp_inbox
  validate_source_ref
  validate_ci

  if deploy_stage_requested; then
    require_env JAILGUN_REMOTE_HOST
    require_env JAILGUN_REMOTE_DIR
    require_env JAILGUN_REMOTE_COMMAND
    validate_remote
  fi
}

build_preflight_done=0
run_build_preflight() {
  if [[ "$build_preflight_done" == "1" ]]; then
    return 0
  fi
  log "building dashboard assets"
  npm --workspace apps/dashboard run build

  log "running CDP recovery smoke"
  npm --workspace apps/chrome-bridge run smoke:cdp-recovery

  log "prebuilding jailgun CLI with $JAILGUN_CARGO_JOBS cargo jobs"
  cargo build --locked --offline -j "$JAILGUN_CARGO_JOBS" -p jailgun-cli --bin jailgun
  build_preflight_done=1
}

wait_for_dashboard() {
  for _ in $(seq 1 "$JAILGUN_DASHBOARD_START_TIMEOUT_SECONDS"); do
    if grep -q 'dashboard listening on http://' "$runner_log" 2>/dev/null; then
      return 0
    fi
    if ! kill -0 "$runner_pid" 2>/dev/null; then
      local early_status
      set +e
      wait "$runner_pid"
      early_status=$?
      set -e
      runner_pid=""
      fail "jailgun run exited before dashboard started (status $early_status)" "$runner_log" "inspect $runner_log"
    fi
    sleep 1
  done
  fail "dashboard did not start before timeout" "$runner_log" "inspect dashboard binding errors in $runner_log"
}

profile_pid_count() {
  local profile_dir="${1:?profile dir required}"
  ps -axo pid=,command= | awk -v profile="$profile_dir" '
    index($0, "--user-data-dir=" profile) > 0 { count++ }
    END { print count + 0 }
  '
}

write_cleanup_proof() {
  local run_dir="${1:?run dir required}"
  local state_dir="${2:?state dir required}"
  local cdp_a="${3:?cdp a required}"
  local cdp_b="${4:?cdp b required}"
  local port_a_listening="false"
  local port_b_listening="false"
  if port_is_listening "$JAILGUN_CDP_HOST" "$JAILGUN_CDP_PORT_START"; then
    port_a_listening="true"
  fi
  if port_is_listening "$JAILGUN_CDP_HOST" "$((JAILGUN_CDP_PORT_START + 1))"; then
    port_b_listening="true"
  fi
  local pids_a
  local pids_b
  pids_a="$(profile_pid_count "$profile_a_dir")"
  pids_b="$(profile_pid_count "$profile_b_dir")"
  cleanup_proof_file="$run_dir/cleanup-proof.json"

  CLEANUP_PROOF_FILE="$cleanup_proof_file" \
  CLEANUP_STATE_DIR="$state_dir" \
  CLEANUP_PROFILE_A="$profile_a_dir" \
  CLEANUP_PROFILE_B="$profile_b_dir" \
  CLEANUP_CDP_A="$cdp_a" \
  CLEANUP_CDP_B="$cdp_b" \
  CLEANUP_PORT_A_LISTENING="$port_a_listening" \
  CLEANUP_PORT_B_LISTENING="$port_b_listening" \
  CLEANUP_PROFILE_A_PID_COUNT="$pids_a" \
  CLEANUP_PROFILE_B_PID_COUNT="$pids_b" \
  node <<'NODE'
const fs = require('node:fs');
const path = require('node:path');

const expectedProfiles = [
  { profile_dir: process.env.CLEANUP_PROFILE_A, cdp_url: process.env.CLEANUP_CDP_A },
  { profile_dir: process.env.CLEANUP_PROFILE_B, cdp_url: process.env.CLEANUP_CDP_B },
];
const proofPath = process.env.CLEANUP_PROOF_FILE;
const statePath = path.join(process.env.CLEANUP_STATE_DIR, 'managed-browsers.json');
const proof = {
  checked_at: new Date().toISOString(),
  state_path: statePath,
  expected_profiles: expectedProfiles,
  ports: {
    [process.env.CLEANUP_CDP_A]: { listening: process.env.CLEANUP_PORT_A_LISTENING === 'true' },
    [process.env.CLEANUP_CDP_B]: { listening: process.env.CLEANUP_PORT_B_LISTENING === 'true' },
  },
  profile_pid_counts: {
    [process.env.CLEANUP_PROFILE_A]: Number(process.env.CLEANUP_PROFILE_A_PID_COUNT || 0),
    [process.env.CLEANUP_PROFILE_B]: Number(process.env.CLEANUP_PROFILE_B_PID_COUNT || 0),
  },
  managed_pool_state_ok: false,
  profile_state_checks: [],
  status: 'failed',
  last_error: null,
};

try {
  const state = JSON.parse(fs.readFileSync(statePath, 'utf8'));
  const profiles = Array.isArray(state.profiles) ? state.profiles : [];
  proof.profile_count = profiles.length;
  proof.profiles = profiles.map((profile) => ({
    slot: profile.slot,
    profile_dir: profile.profile_dir,
    state_dir: profile.state_dir,
    cdp_url: profile.cdp_url,
    pid: profile.pid ?? null,
    started: Boolean(profile.started),
  }));
  proof.managed_pool_state_ok = expectedProfiles.every((expected) => {
    const profile = profiles.find((candidate) =>
      candidate.profile_dir === expected.profile_dir && candidate.cdp_url === expected.cdp_url);
    return profile && profile.started === false && (profile.pid === null || profile.pid === undefined);
  });
  proof.profile_state_checks = expectedProfiles.map((expected) => {
    const profile = profiles.find((candidate) =>
      candidate.profile_dir === expected.profile_dir && candidate.cdp_url === expected.cdp_url);
    if (!profile?.state_dir) {
      return { profile_dir: expected.profile_dir, ok: false, reason: 'missing-profile-state-dir' };
    }
    const profileStatePath = path.join(profile.state_dir, 'managed-browser.json');
    try {
      const profileState = JSON.parse(fs.readFileSync(profileStatePath, 'utf8'));
      return {
        profile_dir: expected.profile_dir,
        path: profileStatePath,
        status: profileState.status ?? '',
        pid: profileState.pid ?? null,
        ok: profileState.status === 'stopped' && (profileState.pid === null || profileState.pid === undefined),
      };
    } catch (error) {
      return {
        profile_dir: expected.profile_dir,
        path: profileStatePath,
        ok: false,
        reason: error instanceof Error ? error.message : String(error),
      };
    }
  });
  const portsOk = Object.values(proof.ports).every((item) => !item.listening);
  const pidsOk = Object.values(proof.profile_pid_counts).every((count) => count === 0);
  const profileStatesOk = proof.profile_state_checks.every((item) => item.ok);
  proof.status = portsOk && pidsOk && proof.managed_pool_state_ok && profileStatesOk
    ? 'success'
    : 'failed';
} catch (error) {
  proof.last_error = error instanceof Error ? error.message : String(error);
}

fs.writeFileSync(proofPath, `${JSON.stringify(proof, null, 2)}\n`);
if (proof.status !== 'success') {
  process.exit(1);
}
NODE
  cleanup_summary="$(node -e 'const fs=require("fs"); const p=JSON.parse(fs.readFileSync(process.argv[1],"utf8")); console.log(`ports_free=${Object.values(p.ports).every((item)=>!item.listening)} profile_pid_counts=${JSON.stringify(p.profile_pid_counts)} managed_pool_state_ok=${p.managed_pool_state_ok} profile_states_ok=${p.profile_state_checks.every((item)=>item.ok)}`); if (p.status !== "success") process.exit(1);' "$cleanup_proof_file")" ||
    fail "cleanup proof did not report success" "$cleanup_proof_file" "stop managed Chrome processes and free CDP ports before rerunning"
}

write_success_receipt() {
  local stage="${1:?stage required}"
  local run_dir="${2:?run dir required}"
  local proof_file="${3:?proof file required}"
  {
    printf 'SUCCESS: live two-profile %s run verified\n' "$stage"
    printf 'run id: %s\n' "$current_run_id"
    printf 'dashboard URL: %s\n' "$dashboard_url"
    printf 'profile dirs: %s %s\n' "$profile_a_dir" "$profile_b_dir"
    printf 'cdp URLs: http://%s:%s http://%s:%s\n' "$JAILGUN_CDP_HOST" "$JAILGUN_CDP_PORT_START" "$JAILGUN_CDP_HOST" "$((JAILGUN_CDP_PORT_START + 1))"
    printf 'fake-chatgpt used: false\n'
    printf 'logs: %s %s\n' "$events_log" "$runner_log"
    printf 'screenshots: %s %s\n' "$run_dir/dashboard-ready.png" "$run_dir/dashboard-final.png"
    printf 'counts: %s\n' "$proof_summary"
    printf 'cleanup: %s\n' "$cleanup_summary"
    printf 'cleanup proof: %s\n' "$cleanup_proof_file"
    printf 'proof: %s\n' "$proof_file"
  } | tee "$receipt_file"
}

run_stage() {
  local stage="${1:?stage required}"
  local tabs="${2:?tabs required}"
  local loops="${3:?loops required}"
  local expect_deploy="${4:?expect deploy required}"
  local planned
  planned="$(planned_tabs_for "$tabs" "$loops")"
  if [[ "$planned" -le 0 ]]; then
    fail "planned tab count must be positive" "tabs=$tabs loops=$loops" "fix stage shape"
  fi

  current_run_id="$RUN_ID_PREFIX-$stage"
  local run_dir="$repo_root/artifacts/live-runs/$current_run_id"
  local runtime_dir="$run_dir/runtime"
  local state_dir="${JAILGUN_CHROME_STATE_DIR:-$run_dir/chrome-state}"
  events_log="$run_dir/events.ndjson"
  runner_log="$run_dir/runner.log"
  monitor_log="$run_dir/monitor.log"
  receipt_file="$run_dir/final-receipt.txt"
  dashboard_url="http://$JAILGUN_DASHBOARD_ADDR"
  mkdir -p "$run_dir" "$runtime_dir" "$state_dir"

  require_port_free "${JAILGUN_DASHBOARD_ADDR%:*}" "${JAILGUN_DASHBOARD_ADDR##*:}" "dashboard"
  require_port_free "$JAILGUN_CDP_HOST" "$JAILGUN_CDP_PORT_START" "CDP profile A"
  require_port_free "$JAILGUN_CDP_HOST" "$((JAILGUN_CDP_PORT_START + 1))" "CDP profile B"
  run_build_preflight

  local cdp_a="http://$JAILGUN_CDP_HOST:$JAILGUN_CDP_PORT_START"
  local cdp_b="http://$JAILGUN_CDP_HOST:$((JAILGUN_CDP_PORT_START + 1))"
  local deploy_flag="--no-deploy"
  if [[ "$expect_deploy" == "true" ]]; then
    deploy_flag="--deploy"
  fi

  local jailgun_cmd=(
    cargo run --locked --offline -j "$JAILGUN_CARGO_JOBS" -p jailgun-cli --bin jailgun --
    run
    --config config/jailgun.example.toml
    --prompt-file "$JAILGUN_PROMPT_FILE"
    --run-id "$current_run_id"
    --tabs "$tabs"
    --loops "$loops"
    --source-repo-url "$JAILGUN_SOURCE_REPO_URL"
    --source-ref "$JAILGUN_SOURCE_REF"
    "$deploy_flag"
    --remote-host "${JAILGUN_REMOTE_HOST:-}"
    --remote-dir "${JAILGUN_REMOTE_DIR:-}"
    --remote-command "${JAILGUN_REMOTE_COMMAND:-}"
    --expected-top-level "$JAILGUN_EXPECTED_TOP_LEVEL"
    --tar-target-name "$JAILGUN_TAR_TARGET_NAME"
    --profile-pool "$profile_a_dir"
    --profile-pool "$profile_b_dir"
    --artifacts-dir "$runtime_dir"
    --downloads-dir "$run_dir/downloads"
    --bridge-env "JAILGUN_CDP_URL="
    --bridge-env "JAILGUN_CDP_HOST=$JAILGUN_CDP_HOST"
    --bridge-env "JAILGUN_CDP_PORT=$JAILGUN_CDP_PORT_START"
    --bridge-env "JAILGUN_CHROME_HEADLESS=false"
    --bridge-env "JAILGUN_CHROME_STATE_DIR=$state_dir"
    --bridge-env "JAILGUN_MESSAGE_STREAM_RETRY_LIMIT=$JAILGUN_MESSAGE_STREAM_RETRY_LIMIT"
    --bridge-env "JAILGUN_MESSAGE_STREAM_RETRY_DELAY_MS=$JAILGUN_MESSAGE_STREAM_RETRY_DELAY_MS"
    --event-buffer 4096
    --deploy-concurrency "$JAILGUN_DEPLOY_CONCURRENCY"
    --serve
    --addr "$JAILGUN_DASHBOARD_ADDR"
    --dashboard-dist apps/dashboard/dist
    --dashboard-hold-seconds "$JAILGUN_DASHBOARD_HOLD_SECONDS"
    --notify-jmcp
    --jmcp-inbox-dir "${JAILGUN_JMCP_INBOX_DIR_RESOLVED:-$JAILGUN_JMCP_INBOX_DIR}"
  )

  if [[ "$JAILGUN_FRESH_SOURCE_CLONE" == "1" ]]; then
    jailgun_cmd+=(--fresh-source-clone)
  fi
  if [[ -n "$JAILGUN_SUBMIT_DELAY_SECONDS" ]]; then
    jailgun_cmd+=(--submit-delay-seconds "$JAILGUN_SUBMIT_DELAY_SECONDS")
  fi
  if [[ -n "$JAILGUN_INITIAL_TAB_BURST" ]]; then
    jailgun_cmd+=(--initial-tab-burst "$JAILGUN_INITIAL_TAB_BURST")
  fi
  if [[ -n "$JAILGUN_SUBMIT_JITTER_PERCENT" ]]; then
    jailgun_cmd+=(--submit-jitter-percent "$JAILGUN_SUBMIT_JITTER_PERCENT")
  elif [[ -n "$JAILGUN_SUBMIT_JITTER_SECONDS" ]]; then
    jailgun_cmd+=(--submit-jitter-seconds "$JAILGUN_SUBMIT_JITTER_SECONDS")
  fi
  if [[ "$JAILGUN_CI" == "1" ]]; then
    jailgun_cmd+=(
      --ci
      --ci-repo "$JAILGUN_CI_REPO"
      --ci-branch "$JAILGUN_CI_BRANCH"
      --ci-max-attempts "$JAILGUN_CI_MAX_ATTEMPTS"
      --ci-poll-seconds "$JAILGUN_CI_POLL_SECONDS"
    )
  fi

  log "starting $stage run $current_run_id tabs=$tabs loops=$loops planned_tabs=$planned deploy=$expect_deploy"
  (env -u JAILGUN_CDP_URL "${jailgun_cmd[@]}" > >(tee "$events_log") 2> >(tee "$runner_log" >&2)) &
  runner_pid=$!

  wait_for_dashboard

  log "starting dashboard monitor for $stage"
  (node scripts/monitor-live-run.mjs \
    --run-id "$current_run_id" \
    --dashboard-url "$dashboard_url" \
    --artifact-dir "$run_dir" \
    --expected-tabs "$planned" \
    --expected-loops "$loops" \
    --expect-deploy "$expect_deploy" \
    --expected-profile-dir "$profile_a_dir" \
    --expected-profile-dir "$profile_b_dir" \
    --expected-cdp-url "$cdp_a" \
    --expected-cdp-url "$cdp_b" \
    --expected-remote-host "${JAILGUN_REMOTE_HOST:-}" \
    --expected-remote-command "${JAILGUN_REMOTE_COMMAND:-}" \
    --max-download-start-latency-ms "$JAILGUN_MAX_DOWNLOAD_START_LATENCY_MS" \
    > >(tee "$monitor_log") \
    2> >(tee -a "$monitor_log" >&2)) &
  monitor_pid=$!

  local runner_status
  local monitor_status
  set +e
  wait "$runner_pid"
  runner_status=$?
  runner_pid=""
  wait "$monitor_pid"
  monitor_status=$?
  monitor_pid=""
  set -e

  if [[ $runner_status -ne 0 ]]; then
    fail "jailgun $stage run failed with status $runner_status" "$runner_log" "inspect $runner_log and $events_log"
  fi
  if [[ $monitor_status -ne 0 ]]; then
    fail "dashboard monitor failed with status $monitor_status" "$run_dir/monitor-proof.json" "inspect $monitor_log and dashboard screenshots in $run_dir"
  fi

  local proof_file="$run_dir/monitor-proof.json"
  if [[ ! -s "$proof_file" ]]; then
    fail "monitor proof was not written" "$proof_file" "inspect $monitor_log"
  fi
  proof_summary="$(node -e 'const fs=require("fs"); const p=JSON.parse(fs.readFileSync(process.argv[1],"utf8")); console.log(`tabs=${p.observed_tabs}/${p.expected_tabs} tab_opened=${p.tab_opened_count} profiles=${JSON.stringify(p.browser_profile_dirs)} slots=${JSON.stringify(p.browser_slots)} cdp=${JSON.stringify(p.browser_cdp_urls)} downloads=${p.download_receipts} receipts=${p.receipt_count} api_receipts=${p.api_receipt_count} closed=${p.tabs_closed} deploy_expected=${p.expect_deploy} deploys=${p.deploy_successes} rate_limit_undismissed=${p.rate_limit_undismissed} errors=${p.error_events}`); if (p.status !== "success") process.exit(1);' "$proof_file")" ||
    fail "monitor proof did not report success" "$proof_file" "inspect $proof_file"

  write_cleanup_proof "$run_dir" "$state_dir" "$cdp_a" "$cdp_b"
  write_success_receipt "$stage" "$run_dir" "$proof_file"
}

validate_static_preflight
if [[ "$preflight_only" == "1" ]]; then
  log "preflight-only checks passed for stage=$stage_mode profile_a=$profile_a_dir profile_b=$profile_b_dir cdp_start=$JAILGUN_CDP_PORT_START"
  exit 0
fi

case "$stage_mode" in
  canary)
    run_stage canary "$JAILGUN_CANARY_TABS" "$JAILGUN_CANARY_LOOPS" false
    ;;
  full)
    run_stage full "$JAILGUN_FULL_TABS" "$JAILGUN_FULL_LOOPS" false
    ;;
  deploy)
    run_stage deploy "$JAILGUN_DEPLOY_TABS" "$JAILGUN_DEPLOY_LOOPS" true
    ;;
  all)
    run_stage canary "$JAILGUN_CANARY_TABS" "$JAILGUN_CANARY_LOOPS" false
    run_stage full "$JAILGUN_FULL_TABS" "$JAILGUN_FULL_LOOPS" false
    if [[ "$JAILGUN_ENABLE_DEPLOY_STAGE" == "1" ]]; then
      run_stage deploy "$JAILGUN_DEPLOY_TABS" "$JAILGUN_DEPLOY_LOOPS" true
    else
      log "deploy stage skipped; rerun --stage deploy --enable-deploy-stage after review to prove deploy"
    fi
    ;;
esac

#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
cd "$repo_root"

env_file="$repo_root/.env.live-7tab"
if [[ ! -f "$env_file" ]]; then
  printf 'FAILED: missing live runner config\nchecked path: %s\nnext action: create %s with the required JAILGUN_* values\n' "$env_file" "$env_file" >&2
  exit 1
fi

set -a
# shellcheck source=/dev/null
source "$env_file"
set +a

log() {
  printf '[live-7tab] %s\n' "$*"
}

fail() {
  local cause="${1:?cause required}"
  local checked="${2:-n/a}"
  local next_action="${3:-inspect $runner_log and $monitor_log}"
  {
    printf 'FAILED: %s\n' "$cause"
    printf 'run id: %s\n' "$RUN_ID"
    printf 'dashboard URL: %s\n' "$dashboard_url"
    printf 'checked: %s\n' "$checked"
    printf 'next action: %s\n' "$next_action"
    printf 'logs: %s %s\n' "$runner_log" "$monitor_log"
  } | tee "$receipt_file" >&2
  exit 1
}

transition_delay_display() {
  local base_seconds="${JAILGUN_SUBMIT_DELAY_SECONDS:-}"
  if [[ -z "$base_seconds" ]] || ! [[ "$base_seconds" =~ ^[0-9]+$ ]]; then
    printf 'config'
    return 0
  fi

  if [[ -n "$JAILGUN_SUBMIT_JITTER_PERCENT" ]]; then
    node -e '
      const base = Number(process.argv[1]);
      const percent = Number(process.argv[2]);
      const delta = Math.floor((base * percent) / 100);
      process.stdout.write(`${base - delta}s..${base + delta}s`);
    ' "$base_seconds" "$JAILGUN_SUBMIT_JITTER_PERCENT"
    return 0
  fi

  if [[ -n "$JAILGUN_SUBMIT_JITTER_SECONDS" ]]; then
    node -e '
      const base = Number(process.argv[1]);
      const jitter = Number(process.argv[2]);
      process.stdout.write(`${base}s..${base + jitter}s`);
    ' "$base_seconds" "$JAILGUN_SUBMIT_JITTER_SECONDS"
    return 0
  fi

  printf '%ss' "$base_seconds"
}

write_success_receipt() {
  {
    printf 'SUCCESS: live 7-tab production run verified\n'
    printf 'run id: %s\n' "$RUN_ID"
    printf 'dashboard URL: %s\n' "$dashboard_url"
    printf 'logs: %s %s\n' "$events_log" "$runner_log"
    printf 'screenshots: %s %s\n' "$run_dir/dashboard-ready.png" "$run_dir/dashboard-final.png"
    printf 'counts: %s\n' "$proof_summary"
    printf 'JMCP enabled: true (inbox %s)\n' "${JAILGUN_JMCP_INBOX_DIR_RESOLVED:-$JAILGUN_JMCP_INBOX_DIR}"
    printf 'proof: %s\n' "$proof_file"
  } | tee "$receipt_file"
}

RUN_ID="${JAILGUN_RUN_ID:-live-7tab-$(date -u +%Y%m%dT%H%M%SZ)}"
JAILGUN_TABS="${JAILGUN_TABS:-7}"
JAILGUN_LOOPS="${JAILGUN_LOOPS:-0}"
JAILGUN_SUBMIT_DELAY_SECONDS="${JAILGUN_SUBMIT_DELAY_SECONDS:-}"
JAILGUN_SUBMIT_JITTER_SECONDS="${JAILGUN_SUBMIT_JITTER_SECONDS:-}"
JAILGUN_SUBMIT_JITTER_PERCENT="${JAILGUN_SUBMIT_JITTER_PERCENT:-}"
JAILGUN_FRESH_SOURCE_CLONE="${JAILGUN_FRESH_SOURCE_CLONE:-0}"
JAILGUN_DASHBOARD_ADDR="${JAILGUN_DASHBOARD_ADDR:-127.0.0.1:8787}"
JAILGUN_DASHBOARD_HOLD_SECONDS="${JAILGUN_DASHBOARD_HOLD_SECONDS:-30}"
JAILGUN_DASHBOARD_KEEP_ALIVE="${JAILGUN_DASHBOARD_KEEP_ALIVE:-0}"
JAILGUN_DASHBOARD_START_TIMEOUT_SECONDS="${JAILGUN_DASHBOARD_START_TIMEOUT_SECONDS:-300}"
JAILGUN_CARGO_JOBS="${JAILGUN_CARGO_JOBS:-10}"
JAILGUN_CI="${JAILGUN_CI:-0}"
JAILGUN_CI_BRANCH="${JAILGUN_CI_BRANCH:-main}"
JAILGUN_CI_MAX_ATTEMPTS="${JAILGUN_CI_MAX_ATTEMPTS:-60}"
JAILGUN_CI_POLL_SECONDS="${JAILGUN_CI_POLL_SECONDS:-20}"
JAILGUN_MAX_DOWNLOAD_START_LATENCY_MS="${JAILGUN_MAX_DOWNLOAD_START_LATENCY_MS:-10000}"
JAILGUN_MESSAGE_STREAM_RETRY_LIMIT="${JAILGUN_MESSAGE_STREAM_RETRY_LIMIT:-6}"
JAILGUN_MESSAGE_STREAM_RETRY_DELAY_MS="${JAILGUN_MESSAGE_STREAM_RETRY_DELAY_MS:-10000}"
JAILGUN_JMCP_INBOX_DIR="${JAILGUN_JMCP_INBOX_DIR:-$HOME/code/jmcp/inbox}"
JAILGUN_JMCP_INBOX_DIR_RESOLVED=""
JAILGUN_CHROME_PROFILE_POOL="${JAILGUN_CHROME_PROFILE_POOL:-}"

run_dir="$repo_root/artifacts/live-runs/$RUN_ID"
runtime_dir="$run_dir/runtime"
events_log="$run_dir/events.ndjson"
runner_log="$run_dir/runner.log"
monitor_log="$run_dir/monitor.log"
receipt_file="$run_dir/final-receipt.txt"
dashboard_url="http://$JAILGUN_DASHBOARD_ADDR"
monitor_pid=""
runner_pid=""

export CARGO_NET_OFFLINE=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tabs)
      JAILGUN_TABS="${2:-}"
      shift 2
      ;;
    --batches)
      batches="${2:-}"
      if ! [[ "$batches" =~ ^[0-9]+$ ]] || [[ "$batches" -le 0 ]]; then
        fail "--batches must be a positive integer" "$batches" "pass --batches 10 for a 70-attempt 7-tab run"
      fi
      JAILGUN_LOOPS="$((batches - 1))"
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
    --fresh-source-clone)
      JAILGUN_FRESH_SOURCE_CLONE=1
      shift
      ;;
    --dashboard-keep-alive)
      JAILGUN_DASHBOARD_KEEP_ALIVE=1
      shift
      ;;
    *)
      fail "unknown argument $1" "$*" "use --tabs, --batches, --submit-delay-seconds, --submit-jitter-seconds, --submit-jitter-percent, --dashboard-keep-alive, and --fresh-source-clone"
      ;;
  esac
done

if ! [[ "$JAILGUN_TABS" =~ ^[0-9]+$ && "$JAILGUN_LOOPS" =~ ^[0-9]+$ ]]; then
  fail "tabs and loops must be non-negative integers" "JAILGUN_TABS=$JAILGUN_TABS JAILGUN_LOOPS=$JAILGUN_LOOPS" "set numeric values in $env_file"
fi
if [[ -n "$JAILGUN_SUBMIT_DELAY_SECONDS" ]] && ! [[ "$JAILGUN_SUBMIT_DELAY_SECONDS" =~ ^[0-9]+$ ]]; then
  fail "submit delay must be a non-negative integer" "$JAILGUN_SUBMIT_DELAY_SECONDS" "pass --submit-delay-seconds 120"
fi
if [[ -n "$JAILGUN_SUBMIT_JITTER_SECONDS" ]] && ! [[ "$JAILGUN_SUBMIT_JITTER_SECONDS" =~ ^[0-9]+$ ]]; then
  fail "submit jitter must be a non-negative integer" "$JAILGUN_SUBMIT_JITTER_SECONDS" "pass --submit-jitter-seconds 0"
fi
if [[ -n "$JAILGUN_SUBMIT_JITTER_PERCENT" ]]; then
  if ! [[ "$JAILGUN_SUBMIT_JITTER_PERCENT" =~ ^[0-9]+$ ]]; then
    fail "submit jitter percent must be a non-negative integer" "$JAILGUN_SUBMIT_JITTER_PERCENT" "pass --submit-jitter-percent 20"
  fi
  if [[ "$JAILGUN_SUBMIT_JITTER_PERCENT" -gt 100 ]]; then
    fail "submit jitter percent must be at most 100" "$JAILGUN_SUBMIT_JITTER_PERCENT" "pass --submit-jitter-percent 20"
  fi
fi
if ! [[ "$JAILGUN_DASHBOARD_KEEP_ALIVE" =~ ^[01]$ ]]; then
  fail "dashboard keep-alive must be 0 or 1" "$JAILGUN_DASHBOARD_KEEP_ALIVE" "pass --dashboard-keep-alive"
fi

JAILGUN_PLANNED_TABS="$(
  node -e '
    const tabs = Number(process.argv[1]);
    const loops = Number(process.argv[2]);
    const planned = tabs * (loops + 1);
    if (!Number.isSafeInteger(tabs) || !Number.isSafeInteger(loops) || !Number.isSafeInteger(planned) || planned <= 0) {
      process.exit(1);
    }
    process.stdout.write(String(planned));
  ' "$JAILGUN_TABS" "$JAILGUN_LOOPS"
)" || fail "could not compute planned tab count" "tabs=$JAILGUN_TABS loops=$JAILGUN_LOOPS" "check for overflow or zero-sized runs"
export JAILGUN_PLANNED_TABS

mkdir -p "$run_dir" "$runtime_dir"
export CARGO_TARGET_DIR="$run_dir/cargo-target"

JAILGUN_BATCHES="$((JAILGUN_LOOPS + 1))"
dashboard_keep_alive_display="false"
if [[ "$JAILGUN_DASHBOARD_KEEP_ALIVE" == "1" ]]; then
  dashboard_keep_alive_display="true"
fi
log "run config tabs=$JAILGUN_TABS batches=$JAILGUN_BATCHES loops=$JAILGUN_LOOPS planned_tabs=$JAILGUN_PLANNED_TABS transition_delay=$(transition_delay_display) dashboard_keep_alive=$dashboard_keep_alive_display fresh_source_clone=$JAILGUN_FRESH_SOURCE_CLONE"

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

check_port_free() {
  local host="${JAILGUN_DASHBOARD_ADDR%:*}"
  local port="${JAILGUN_DASHBOARD_ADDR##*:}"
  if command -v lsof >/dev/null 2>&1 && lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1; then
    fail "dashboard port is occupied" "$JAILGUN_DASHBOARD_ADDR" "free TCP port $port or set JAILGUN_DASHBOARD_ADDR to an unused address"
  fi
  if command -v nc >/dev/null 2>&1 && nc -z "$host" "$port" >/dev/null 2>&1; then
    fail "dashboard port is reachable before runner start" "$JAILGUN_DASHBOARD_ADDR" "stop the process already serving $JAILGUN_DASHBOARD_ADDR"
  fi
}

validate_jmcp_inbox() {
  if [[ "${JAILGUN_NOTIFY_JMCP:-}" != "1" ]]; then
    fail "JMCP notifications are not enabled" "JAILGUN_NOTIFY_JMCP=${JAILGUN_NOTIFY_JMCP:-unset}" "set JAILGUN_NOTIFY_JMCP=1 in $env_file"
  fi
  local inbox_dir="${JAILGUN_JMCP_INBOX_DIR:-$HOME/code/jmcp/inbox}"
  case "$inbox_dir" in
    "~"|"~/"*) inbox_dir="${HOME}${inbox_dir#~}" ;;
  esac
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
    fail "remote checkout is dirty before production" "$JAILGUN_REMOTE_HOST:$JAILGUN_REMOTE_DIR" "preserve or reset the remote changes, then rerun; dirty files: $output"
  fi
  if [[ $status -ne 0 ]]; then
    fail "SSH remote checkout is not ready" "$JAILGUN_REMOTE_HOST:$JAILGUN_REMOTE_DIR" "verify SSH access, origin/main, git, bash, and the remote directory; ssh output: $output"
  fi
}

validate_ci() {
  if [[ "$JAILGUN_CI" == "1" ]]; then
    require_env JAILGUN_CI_REPO
  fi
}

run_preflight() {
  require_env JAILGUN_CDP_URL
  require_env JAILGUN_PROMPT_FILE
  require_env JAILGUN_SOURCE_REPO_URL
  require_env JAILGUN_SOURCE_REF
  require_env JAILGUN_REMOTE_HOST
  require_env JAILGUN_REMOTE_DIR
  require_env JAILGUN_REMOTE_COMMAND
  require_env JAILGUN_EXPECTED_TOP_LEVEL
  require_env JAILGUN_TAR_TARGET_NAME
  require_file "$JAILGUN_PROMPT_FILE" "prompt file"
  check_port_free
  validate_jmcp_inbox
  validate_source_ref
  validate_remote
  validate_ci

  if [[ -n "$JAILGUN_CHROME_PROFILE_POOL" ]]; then
    IFS=':' read -ra _pool_dirs <<< "$JAILGUN_CHROME_PROFILE_POOL"
    for _pdir in "${_pool_dirs[@]}"; do
      _pdir_expanded="$_pdir"
      case "$_pdir_expanded" in
        "~"|"~/"*) _pdir_expanded="${HOME}${_pdir_expanded#~}" ;;
      esac
      if [[ ! -d "$_pdir_expanded" ]]; then
        fail "profile pool dir does not exist" "$_pdir_expanded" "create the profile dir or update JAILGUN_CHROME_PROFILE_POOL"
      fi
    done
    log "profile pool: ${_pool_dirs[*]}"
  fi

  log "building dashboard assets"
  npm --workspace apps/dashboard run build

  log "running CDP recovery smoke"
  npm --workspace apps/chrome-bridge run smoke:cdp-recovery

  log "prebuilding jailgun CLI with $JAILGUN_CARGO_JOBS cargo jobs"
  cargo build --locked --offline -j "$JAILGUN_CARGO_JOBS" -p jailgun-cli --bin jailgun
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

run_preflight

jailgun_cmd=(
  cargo run --locked --offline -j "$JAILGUN_CARGO_JOBS" -p jailgun-cli --bin jailgun --
  run
  --config config/jailgun.example.toml
  --prompt-file "$JAILGUN_PROMPT_FILE"
  --run-id "$RUN_ID"
  --tabs "$JAILGUN_TABS"
  --loops "$JAILGUN_LOOPS"
  --source-repo-url "$JAILGUN_SOURCE_REPO_URL"
  --source-ref "$JAILGUN_SOURCE_REF"
  --deploy
  --remote-host "$JAILGUN_REMOTE_HOST"
  --remote-dir "$JAILGUN_REMOTE_DIR"
  --remote-command "$JAILGUN_REMOTE_COMMAND"
  --expected-top-level "$JAILGUN_EXPECTED_TOP_LEVEL"
  --tar-target-name "$JAILGUN_TAR_TARGET_NAME"
  --artifacts-dir "$runtime_dir"
  --downloads-dir "$run_dir/downloads"
  --event-buffer 4096
  --serve
  --addr "$JAILGUN_DASHBOARD_ADDR"
  --dashboard-dist apps/dashboard/dist
  --dashboard-hold-seconds "$JAILGUN_DASHBOARD_HOLD_SECONDS"
  --notify-jmcp
  --jmcp-inbox-dir "${JAILGUN_JMCP_INBOX_DIR_RESOLVED:-$JAILGUN_JMCP_INBOX_DIR}"
)

if [[ -n "$JAILGUN_CHROME_PROFILE_POOL" ]]; then
  IFS=':' read -ra _pool_dirs <<< "$JAILGUN_CHROME_PROFILE_POOL"
  for _pdir in "${_pool_dirs[@]}"; do
    _pdir_expanded="$_pdir"
    case "$_pdir_expanded" in
      "~"|"~/"*) _pdir_expanded="${HOME}${_pdir_expanded#~}" ;;
    esac
    jailgun_cmd+=(--profile-pool "$_pdir_expanded")
  done
fi

if [[ -n "$JAILGUN_SUBMIT_DELAY_SECONDS" ]]; then
  jailgun_cmd+=(--submit-delay-seconds "$JAILGUN_SUBMIT_DELAY_SECONDS")
fi
if [[ -n "$JAILGUN_SUBMIT_JITTER_PERCENT" ]]; then
  jailgun_cmd+=(--submit-jitter-percent "$JAILGUN_SUBMIT_JITTER_PERCENT")
elif [[ -n "$JAILGUN_SUBMIT_JITTER_SECONDS" ]]; then
  jailgun_cmd+=(--submit-jitter-seconds "$JAILGUN_SUBMIT_JITTER_SECONDS")
fi
if [[ "$JAILGUN_FRESH_SOURCE_CLONE" == "1" ]]; then
  jailgun_cmd+=(--fresh-source-clone)
fi
if [[ "$JAILGUN_DASHBOARD_KEEP_ALIVE" == "1" ]]; then
  jailgun_cmd+=(--dashboard-keep-alive)
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

log "starting production run $RUN_ID"
("${jailgun_cmd[@]}" > >(tee "$events_log") 2> >(tee "$runner_log" >&2)) &
runner_pid=$!

wait_for_dashboard

log "starting dashboard monitor"
(node scripts/monitor-live-run.mjs \
  --run-id "$RUN_ID" \
  --dashboard-url "$dashboard_url" \
  --artifact-dir "$run_dir" \
  --expected-tabs "$JAILGUN_PLANNED_TABS" \
  --expected-remote-host "$JAILGUN_REMOTE_HOST" \
  --expected-remote-command "$JAILGUN_REMOTE_COMMAND" \
  --max-download-start-latency-ms "$JAILGUN_MAX_DOWNLOAD_START_LATENCY_MS" \
  > >(tee "$monitor_log") \
  2> >(tee -a "$monitor_log" >&2)) &
monitor_pid=$!

if [[ "$JAILGUN_DASHBOARD_KEEP_ALIVE" == "1" ]]; then
  set +e
  wait "$monitor_pid"
  monitor_status=$?
  monitor_pid=""
  set -e

  if [[ $monitor_status -ne 0 ]]; then
    fail "dashboard monitor failed with status $monitor_status" "$run_dir/monitor-proof.json" "inspect $monitor_log and dashboard screenshots in $run_dir"
  fi

  proof_file="$run_dir/monitor-proof.json"
  if [[ ! -s "$proof_file" ]]; then
    fail "monitor proof was not written" "$proof_file" "inspect $monitor_log"
  fi

  if [[ "$JAILGUN_FRESH_SOURCE_CLONE" == "1" ]]; then
    node -e 'const fs=require("fs"); const p=JSON.parse(fs.readFileSync(process.argv[1],"utf8")); const expected=Number(process.argv[2]); if (p.fresh_source_clones !== expected) process.exit(1);' "$proof_file" "$JAILGUN_PLANNED_TABS" ||
      fail "fresh source clone path was not proven for every tab" "$proof_file" "inspect source-upload events and $runner_log"
    if ! grep -q 'fresh_source_clone=true' "$runner_log" || ! grep -q 'clone_dir=' "$runner_log"; then
      fail "fresh source clone fields are missing from runner log" "$runner_log" "inspect bridge source-upload stderr lines"
    fi
  fi

  proof_summary="$(node -e 'const fs=require("fs"); const p=JSON.parse(fs.readFileSync(process.argv[1],"utf8")); console.log(`tabs=${p.observed_tabs}/${p.expected_tabs} loops=${p.loop_count ?? 0} planned_tabs=${p.planned_tabs ?? p.expected_tabs} loops_remaining=${p.loops_remaining ?? 0} download_started=${p.download_started} download_start_within_10s=${p.download_start_within_10s} downloads=${p.download_receipts} stopped=${p.generation_stopped} early_stops=${p.early_stops_succeeded ?? 0}/${p.early_stops_attempted ?? 0} closed=${p.tabs_closed} fresh_downloads=${p.fresh_downloads} fresh_source_clones=${p.fresh_source_clones ?? 0} deploys=${p.deploy_successes} files_changed=${p.files_changed ?? 0} additions=${p.additions ?? 0} deletions=${p.deletions ?? 0} local_tests_passed=${p.local_tests_passed ?? 0} remote_tests_passed=${p.remote_tests_passed ?? 0} remote_host=${p.remote_host_matches} remote_command=${p.remote_command_matches} remote_local_ci_passed=${p.remote_local_ci_passed ?? 0} github_ci_passed=${p.ci_passed} github_ci_skipped=${p.ci_skipped} rate_limit_detections=${p.rate_limit_detections ?? 0} rate_limit_dismissed=${p.rate_limit_dismissed ?? 0} rate_limit_undismissed=${p.rate_limit_undismissed ?? 0} errors=${p.error_events}`); if (p.status !== "success") process.exit(1);' "$proof_file")" ||
    fail "monitor proof did not report success" "$proof_file" "inspect $proof_file"

  write_success_receipt
  log "dashboard keep-alive active; press Ctrl-C to stop"
  wait "$runner_pid"
  fail "dashboard keep-alive runner exited before Ctrl-C" "$runner_log" "keep the dashboard attached until Ctrl-C"
else
  set +e
  wait "$runner_pid"
  runner_status=$?
  runner_pid=""
  wait "$monitor_pid"
  monitor_status=$?
  monitor_pid=""
  set -e

  if [[ $runner_status -ne 0 ]]; then
    fail "jailgun production run failed with status $runner_status" "$runner_log" "inspect $runner_log and $events_log"
  fi

  if [[ $monitor_status -ne 0 ]]; then
    fail "dashboard monitor failed with status $monitor_status" "$run_dir/monitor-proof.json" "inspect $monitor_log and dashboard screenshots in $run_dir"
  fi

  proof_file="$run_dir/monitor-proof.json"
  if [[ ! -s "$proof_file" ]]; then
    fail "monitor proof was not written" "$proof_file" "inspect $monitor_log"
  fi

  if [[ "$JAILGUN_FRESH_SOURCE_CLONE" == "1" ]]; then
    node -e 'const fs=require("fs"); const p=JSON.parse(fs.readFileSync(process.argv[1],"utf8")); const expected=Number(process.argv[2]); if (p.fresh_source_clones !== expected) process.exit(1);' "$proof_file" "$JAILGUN_PLANNED_TABS" ||
      fail "fresh source clone path was not proven for every tab" "$proof_file" "inspect source-upload events and $runner_log"
    if ! grep -q 'fresh_source_clone=true' "$runner_log" || ! grep -q 'clone_dir=' "$runner_log"; then
      fail "fresh source clone fields are missing from runner log" "$runner_log" "inspect bridge source-upload stderr lines"
    fi
  fi

  proof_summary="$(node -e 'const fs=require("fs"); const p=JSON.parse(fs.readFileSync(process.argv[1],"utf8")); console.log(`tabs=${p.observed_tabs}/${p.expected_tabs} loops=${p.loop_count ?? 0} planned_tabs=${p.planned_tabs ?? p.expected_tabs} loops_remaining=${p.loops_remaining ?? 0} download_started=${p.download_started} download_start_within_10s=${p.download_start_within_10s} downloads=${p.download_receipts} stopped=${p.generation_stopped} early_stops=${p.early_stops_succeeded ?? 0}/${p.early_stops_attempted ?? 0} closed=${p.tabs_closed} fresh_downloads=${p.fresh_downloads} fresh_source_clones=${p.fresh_source_clones ?? 0} deploys=${p.deploy_successes} files_changed=${p.files_changed ?? 0} additions=${p.additions ?? 0} deletions=${p.deletions ?? 0} local_tests_passed=${p.local_tests_passed ?? 0} remote_tests_passed=${p.remote_tests_passed ?? 0} remote_host=${p.remote_host_matches} remote_command=${p.remote_command_matches} remote_local_ci_passed=${p.remote_local_ci_passed ?? 0} github_ci_passed=${p.ci_passed} github_ci_skipped=${p.ci_skipped} rate_limit_detections=${p.rate_limit_detections ?? 0} rate_limit_dismissed=${p.rate_limit_dismissed ?? 0} rate_limit_undismissed=${p.rate_limit_undismissed ?? 0} errors=${p.error_events}`); if (p.status !== "success") process.exit(1);' "$proof_file")" ||
    fail "monitor proof did not report success" "$proof_file" "inspect $proof_file"

  write_success_receipt
fi

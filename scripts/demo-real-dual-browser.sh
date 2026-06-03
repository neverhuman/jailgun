#!/usr/bin/env bash
# Real production dual-browser demo.
#
# Pre-conditions:
#   1. ~/.jailgun/profiles/prod-google-a + prod-google-b exist with persisted
#      chatgpt.com login cookies. Run
#      `bash scripts/login-profile-interactive.sh ~/.jailgun/profiles/prod-google-a`
#      (and again for prod-google-b) first.
#   2. .env.live-7tab has JAILGUN_REMOTE_HOST/DIR/COMMAND/SOURCE_REPO_URL
#      set so the deploy stage can SSH + push.
#
# What this does:
#   - Loads .env.live-7tab.
#   - Runs `jailgun run --tabs 2 --profile-pool A --profile-pool B
#     --deploy --serve --notify-jmcp` against the REAL chatgpt.com.
#   - Tail events.ndjson live so you can watch tab-opened / prompt
#     submitted / tar-discovered / download-receipt / deploy-finished
#     in real time.
#   - On success: leaves both Chromes alive so the next run attaches
#     and reuses cookies. Print artifacts dir + dashboard URL.
#   - On error / Ctrl-C: force-kills any Chromes spawned from
#     ~/.jailgun/profiles/google-*.
#
# Env overrides:
#   TABS               total tabs (default 2; round-robin across pool)
#   LOOPS              extra batches (default 0; 1 means tabs * 2)
#   PROMPT_FILE        prompt file path (default prompts/templates/harden-repo.txt)
#   DASHBOARD_ADDR     dashboard bind (default 127.0.0.1:8787)
#   SKIP_JMCP          set to 1 to skip --notify-jmcp
#   INITIAL_TAB_BURST  tabs to queue immediately (default: TABS)

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd -P)"
cd "$repo_root"

PROFILE_ROOT="$HOME/.jailgun/profiles"
PROFILE_A="${PROFILE_A:-$PROFILE_ROOT/prod-google-a}"
PROFILE_B="${PROFILE_B:-$PROFILE_ROOT/prod-google-b}"
ENV_FILE="$repo_root/.env.live-7tab"

profile_pids() {
  local profile="${1:?profile required}"
  ps -axo pid=,command= | awk -v profile="$profile" '
    index($0, "--user-data-dir=" profile) > 0 { print $1 }
  '
}

profile_pid_count() {
  local profile="${1:?profile required}"
  local pids
  pids="$(profile_pids "$profile")"
  if [[ -z "$pids" ]]; then
    printf '0\n'
  else
    printf '%s\n' "$pids" | wc -l | tr -d ' '
  fi
}

kill_profile_chromes() {
  local signal="${1:?signal required}"
  for profile in "$PROFILE_A" "$PROFILE_B"; do
    local pids
    pids="$(profile_pids "$profile")"
    [[ -n "$pids" ]] || continue
    while IFS= read -r pid; do
      [[ -n "$pid" ]] || continue
      kill "-$signal" "$pid" 2>/dev/null || true
    done <<<"$pids"
  done
}

if [[ ! -f "$ENV_FILE" ]]; then
  printf '[demo] missing %s — create it (or run with the right repo)\n' \
    "$ENV_FILE" >&2
  exit 1
fi

for profile in "$PROFILE_A" "$PROFILE_B"; do
  cookies="$profile/Default/Cookies"
  if [[ ! -s "$cookies" ]]; then
    printf '[demo] missing or empty cookies at %s\n' "$cookies" >&2
    printf '       run: bash scripts/login-profile-interactive.sh %s\n' \
      "$profile" >&2
    exit 1
  fi
done

# shellcheck source=/dev/null
set -a
source "$ENV_FILE"
set +a

# .env.live-7tab has a known typo (JAILGUN_CDP_URL=...:922 missing a digit).
# Force the bridge back to its default 9224 / 9225 base so managed Chrome
# gets sane ports.
unset JAILGUN_CDP_URL

# Stale SingletonSocket / Lock / Cookie from a previously crashed Chrome
# will block the bridge from launching a fresh managed Chrome on the
# profile. Remove them if no process currently uses the profile dir.
for profile in "$PROFILE_A" "$PROFILE_B"; do
  alive="$(profile_pid_count "$profile")"
  if (( alive == 0 )); then
    rm -f "$profile/SingletonSocket" "$profile/SingletonLock" "$profile/SingletonCookie"
  fi
done

for var in JAILGUN_REMOTE_HOST JAILGUN_REMOTE_DIR JAILGUN_REMOTE_COMMAND \
           JAILGUN_SOURCE_REPO_URL JAILGUN_EXPECTED_TOP_LEVEL \
           JAILGUN_TAR_TARGET_NAME; do
  if [[ -z "${!var:-}" ]]; then
    printf '[demo] %s not set in %s\n' "$var" "$ENV_FILE" >&2
    exit 1
  fi
done

TABS="${TABS:-2}"
LOOPS="${LOOPS:-0}"
INITIAL_TAB_BURST="${INITIAL_TAB_BURST:-$TABS}"
PROMPT_FILE="${PROMPT_FILE:-prompts/templates/harden-repo.txt}"
DASHBOARD_ADDR="${DASHBOARD_ADDR:-127.0.0.1:8787}"
if [[ ! -f "$PROMPT_FILE" ]]; then
  printf '[demo] missing prompt file: %s\n' "$PROMPT_FILE" >&2
  exit 1
fi

RUN_ID="real-dual-$(date -u +%Y%m%dT%H%M%SZ)"
ARTIFACTS_DIR="$repo_root/artifacts/live-runs/$RUN_ID"
EVENTS_LOG="$ARTIFACTS_DIR/events.ndjson"
RUNNER_LOG="$ARTIFACTS_DIR/runner.log"
mkdir -p "$ARTIFACTS_DIR"

runner_pid=""
tail_pid=""

cleanup() {
  local status=$?
  if [[ -n "$tail_pid" ]] && kill -0 "$tail_pid" 2>/dev/null; then
    kill "$tail_pid" 2>/dev/null || true
  fi
  if [[ -n "$runner_pid" ]] && kill -0 "$runner_pid" 2>/dev/null; then
    kill -TERM "$runner_pid" 2>/dev/null || true
    sleep 2
    kill -KILL "$runner_pid" 2>/dev/null || true
  fi

  if (( status != 0 )); then
    printf '[demo] non-zero exit (%s) · nuking profile Chromes for safety\n' \
      "$status" >&2
    kill_profile_chromes TERM
    sleep 1
    kill_profile_chromes KILL
  fi

  exit "$status"
}
trap cleanup EXIT INT TERM

printf '[demo] run id    : %s\n' "$RUN_ID"
printf '[demo] artifacts : %s\n' "$ARTIFACTS_DIR"
printf '[demo] tabs/loops: %s / %s\n' "$TABS" "$LOOPS"
printf '[demo] prompt    : %s\n' "$PROMPT_FILE"
printf '[demo] dashboard : http://%s\n' "$DASHBOARD_ADDR"
printf '[demo] profiles  : %s · %s\n' "$PROFILE_A" "$PROFILE_B"
printf '[demo] remote    : %s%s\n' "$JAILGUN_REMOTE_HOST" "$JAILGUN_REMOTE_DIR"
printf '\n'

jailgun_cmd=(
  cargo run --locked -p jailgun-cli --bin jailgun --
  run
  --config "config/jailgun.example.toml"
  --prompt-file "$PROMPT_FILE"
  --run-id "$RUN_ID"
  --tabs "$TABS"
  --loops "$LOOPS"
  --profile-pool "$PROFILE_A"
  --profile-pool "$PROFILE_B"
  --source-repo-url "$JAILGUN_SOURCE_REPO_URL"
  --source-ref "${JAILGUN_SOURCE_REF:-HEAD}"
  --deploy
  --remote-host "$JAILGUN_REMOTE_HOST"
  --remote-dir "$JAILGUN_REMOTE_DIR"
  --remote-command "$JAILGUN_REMOTE_COMMAND"
  --submit-delay-seconds 0
  --submit-jitter-seconds 0
  --initial-tab-burst "$INITIAL_TAB_BURST"
  --expected-top-level "$JAILGUN_EXPECTED_TOP_LEVEL"
  --tar-target-name "$JAILGUN_TAR_TARGET_NAME"
  --artifacts-dir "$ARTIFACTS_DIR"
  --downloads-dir "$ARTIFACTS_DIR/downloads"
  --event-buffer 1024
  --serve
  --addr "$DASHBOARD_ADDR"
  --dashboard-hold-seconds 30
)

if [[ "${SKIP_JMCP:-0}" != "1" ]]; then
  jailgun_cmd+=(
    --notify-jmcp
    --jmcp-inbox-dir "${JAILGUN_JMCP_INBOX_DIR:-$HOME/code/jmcp/inbox}"
  )
fi

printf '[demo] launching jailgun (log → %s)\n' "$RUNNER_LOG"
"${jailgun_cmd[@]}" \
  > >(tee "$RUNNER_LOG") \
  2> >(tee -a "$RUNNER_LOG" >&2) &
runner_pid=$!

printf '[demo] tailing events.ndjson when it appears\n'
( while [[ ! -s "$EVENTS_LOG" ]] && kill -0 "$runner_pid" 2>/dev/null; do
    sleep 1
  done
  if [[ -s "$EVENTS_LOG" ]]; then
    tail -F "$EVENTS_LOG" 2>/dev/null
  fi ) &
tail_pid=$!

wait "$runner_pid"
runner_exit=$?

if [[ -n "$tail_pid" ]] && kill -0 "$tail_pid" 2>/dev/null; then
  kill "$tail_pid" 2>/dev/null || true
fi
tail_pid=""

printf '\n[demo] runner exited with status %s\n' "$runner_exit"

if (( runner_exit == 0 )); then
  alive=$(( $(profile_pid_count "$PROFILE_A") + $(profile_pid_count "$PROFILE_B") ))
  printf '[demo] DONE · jailgun Chrome procs left alive (for reuse): %s\n' "$alive"
  printf '[demo] artifacts: %s\n' "$ARTIFACTS_DIR"
  printf '[demo] dashboard: http://%s\n' "$DASHBOARD_ADDR"
  printf '[demo] re-run     : bash scripts/demo-real-dual-browser.sh\n'
  printf '[demo] cleanup all: bash scripts/cleanup-profile-chromes.sh\n'
fi

exit "$runner_exit"

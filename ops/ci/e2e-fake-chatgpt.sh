#!/usr/bin/env bash
# End-to-end CI lane: drive the full jailgun stack against the
# fake-chatgpt simulator. No real ChatGPT session burned; no SSH; no
# Telegram. Asserts the event stream produces the kinds we expect for a
# one-tab single-loop smoke.

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=ops/ci/lib.sh
source "$script_dir/lib.sh"
ci_enter_repo_root "$script_dir"
ci_require_cmd node
ci_require_cmd cargo

ARTIFACTS_DIR="${E2E_FAKE_CHATGPT_DIR:-target/e2e-fake-chatgpt}"
PORT="${FAKE_CHATGPT_PORT:-8082}"
PROFILE_DIR="$ARTIFACTS_DIR/chrome-profile"
DOWNLOADS_DIR="$ARTIFACTS_DIR/downloads"
RECEIPTS_DIR="$ARTIFACTS_DIR/receipts"
EVENTS_LOG="$ARTIFACTS_DIR/events.ndjson"
CONFIG_PATH="$ARTIFACTS_DIR/jailgun.fake-chatgpt.toml"
PROMPT_PATH="$ARTIFACTS_DIR/prompt.txt"
RUN_ID="e2e-fake-chatgpt-$(date -u +%Y%m%dT%H%M%SZ)"

rm -rf "$ARTIFACTS_DIR"
mkdir -p "$ARTIFACTS_DIR" "$PROFILE_DIR" "$DOWNLOADS_DIR" "$RECEIPTS_DIR"

cat > "$CONFIG_PATH" <<EOF
[project]
name = "jailgun-e2e-fake-chatgpt"
repository = "git@example.com:org/jailgun.git"

[browser]
chat_url = "http://127.0.0.1:${PORT}/"
model = "pro-extended"
tabs = 1
poll_interval_seconds = 1
completion_check_seconds = 1
submit_delay_seconds = 0
submit_jitter_seconds = 0
tar_wait_minutes = 1
profile_dir_env = "JAILGUN_CHROME_PROFILE_DIR"
state_dir_env = "JAILGUN_CHROME_STATE_DIR"

[paths]
artifacts_dir = "${ARTIFACTS_DIR}"
downloads_dir_env = "JAILGUN_DOWNLOADS_DIR"

[source_archive]
enabled = false
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
node scripts/start-fake-chatgpt-with-cdp.mjs > "$ARTIFACTS_DIR/fake-chatgpt.log" 2>&1 &
fake_pid=$!
trap 'kill $fake_pid 2>/dev/null || true' EXIT

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

ci_log "running 1-tab jailgun smoke against fake-chatgpt"
set +e
JAILGUN_CHROME_PROFILE_DIR="$PROFILE_DIR" \
JAILGUN_DOWNLOADS_DIR="$DOWNLOADS_DIR" \
JAILGUN_ARTIFACTS_DIR="$ARTIFACTS_DIR" \
JAILGUN_SOURCE_REPO_URL="$(pwd)" \
JAILGUN_REMOTE_HOST="example.invalid" \
JAILGUN_REMOTE_DIR="/tmp/jailgun-e2e-fake" \
JAILGUN_REMOTE_COMMAND="true" \
cargo run --locked --quiet -p jailgun-cli --bin jailgun -- run \
  --config "$CONFIG_PATH" \
  --prompt-file "$PROMPT_PATH" \
  --run-id "$RUN_ID" \
  --tabs 1 \
  --no-deploy \
  --artifacts-dir "$ARTIFACTS_DIR" \
  --downloads-dir "$DOWNLOADS_DIR" \
  --event-buffer 256 \
  > "$ARTIFACTS_DIR/runner.log" 2> >(tee -a "$ARTIFACTS_DIR/runner.log" >&2) &
runner_pid=$!
runner_exit=
(sleep 180 && kill "$runner_pid" 2>/dev/null) &
watchdog_pid=$!

wait "$runner_pid"
runner_exit=$?
kill "$watchdog_pid" 2>/dev/null || true
set -e

ci_log "runner exited with status ${runner_exit}"

if [[ -s "$ARTIFACTS_DIR/events.ndjson" ]]; then
  EVENTS_LOG="$ARTIFACTS_DIR/events.ndjson"
else
  EVENTS_LOG="$ARTIFACTS_DIR/runner.log"
fi

required_kinds=("run-started" "tab-opened")
for kind in "${required_kinds[@]}"; do
  if ! grep -q "\"$kind\"" "$EVENTS_LOG"; then
    ci_log "missing required event kind: $kind"
    tail -50 "$EVENTS_LOG" >&2 || true
    exit 1
  fi
done

ci_log "e2e-fake-chatgpt lane green (events: $(wc -l < "$EVENTS_LOG" | tr -d ' ') lines)"

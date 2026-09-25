use crate::{
    job::JobSpec,
    util::{sanitize_ref_fragment, shell_quote},
};

use super::LAUNCHER_SCHEMA_VERSION;

/// Build the on-remote bash launcher for one tab's deploy job.
pub fn build_launcher_script(spec: &JobSpec) -> String {
    let job_id = format!(
        "{}-tab-{:02}",
        sanitize_ref_fragment(&spec.run_id),
        spec.tab_id
    );
    let stash_flag = if spec.stash_on_failure { "1" } else { "0" };
    format!(
        r#"#!/usr/bin/env bash
# jailgun-launcher schema v{schema} (generated)
set -euo pipefail
umask 077

REMOTE_DIR={remote_dir}
ARCHIVE_PATH={archive_path}
REMOTE_COMMAND={remote_command}
STRIP_COMPONENTS={strip_components}
LOCAL_SHA256={local_sha256}
REMOTE_SHA256={remote_sha256}
RUN_ID={run_id}
TAB_INDEX={tab_index}
JOB_ID={job_id}
STASH_ON_FAILURE={stash}

JOB_DIR="/tmp/jailgun-runs/$JOB_ID"
STATUS_PATH="$JOB_DIR/status.json"
LOG_PATH="$JOB_DIR/launch.log"
FAILURE_MARKER="$JOB_DIR/deploy.failed"
PATCH_DIR="$JOB_DIR/failed"
mkdir -p "$JOB_DIR" "$PATCH_DIR"

PRE_HEAD=""
POST_HEAD=""
EXIT_CODE=""
FAILURE_REASON=""
PRESERVED_REF=""
PRESERVED_SHA=""
PRESERVED_STASH=""
PRESERVED_STASH_REF=""
PRESERVED_PATCH=""
RESET_TO=""
RESET_OK=""
FILES_CHANGED=""
ADDITIONS=""
DELETIONS=""
TOP_PATHS_JSON="[]"
STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
FINISHED_AT=""
FAILED_AT=""

json_escape() {{
  printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' -e ':a;N;$!ba;s/\n/\\n/g'
}}

opt_string() {{
  if [ -z "$1" ]; then
    printf 'null'
  else
    printf '"%s"' "$(json_escape "$1")"
  fi
}}

opt_int() {{
  if [ -z "$1" ]; then
    printf 'null'
  else
    printf '%s' "$1"
  fi
}}

opt_bool() {{
  case "$1" in
    "") printf 'null' ;;
    true|TRUE|1) printf 'true' ;;
    *) printf 'false' ;;
  esac
}}

write_status() {{
  local PHASE="$1"
  local TMP="$STATUS_PATH.tmp"
  {{
    printf '{{'
    printf '"schema_version":%s,' '{schema}'
    printf '"phase":"%s",' "$PHASE"
    printf '"exit_code":%s,' "$(opt_int "$EXIT_CODE")"
    printf '"pre_head":%s,' "$(opt_string "$PRE_HEAD")"
    printf '"post_head":%s,' "$(opt_string "$POST_HEAD")"
    printf '"preserved_ref":%s,' "$(opt_string "$PRESERVED_REF")"
    printf '"preserved_sha":%s,' "$(opt_string "$PRESERVED_SHA")"
    printf '"preserved_stash":%s,' "$(opt_string "$PRESERVED_STASH")"
    printf '"preserved_stash_ref":%s,' "$(opt_string "$PRESERVED_STASH_REF")"
    printf '"preserved_patch_path":%s,' "$(opt_string "$PRESERVED_PATCH")"
    printf '"reset_to":%s,' "$(opt_string "$RESET_TO")"
    printf '"reset_ok":%s,' "$(opt_bool "$RESET_OK")"
    printf '"failure_reason":%s,' "$(opt_string "$FAILURE_REASON")"
    printf '"files_changed":%s,' "$(opt_int "$FILES_CHANGED")"
    printf '"additions":%s,' "$(opt_int "$ADDITIONS")"
    printf '"deletions":%s,' "$(opt_int "$DELETIONS")"
    printf '"top_paths":%s,' "$TOP_PATHS_JSON"
    printf '"started_at":%s,' "$(opt_string "$STARTED_AT")"
    printf '"finished_at":%s,' "$(opt_string "$FINISHED_AT")"
    printf '"failed_at":%s' "$(opt_string "$FAILED_AT")"
    printf '}}\n'
  }} > "$TMP"
  mv "$TMP" "$STATUS_PATH"
}}

write_failure_marker() {{
  {{
    printf 'failed_at=%s\n' "$FAILED_AT"
    printf 'run_id=%s\n' "$RUN_ID"
    printf 'tab_index=%s\n' "$TAB_INDEX"
    printf 'reason=%s\n' "$FAILURE_REASON"
    printf 'exit_code=%s\n' "${{EXIT_CODE:-unknown}}"
    printf 'archive=%s\n' "$ARCHIVE_PATH"
    printf 'preserved_ref=%s\n' "$PRESERVED_REF"
    printf 'preserved_stash_ref=%s\n' "$PRESERVED_STASH_REF"
  }} > "$FAILURE_MARKER"
}}

git_head() {{ git rev-parse HEAD 2>/dev/null || true; }}
git_dirty() {{ test -n "$(git status --porcelain 2>/dev/null || true)"; }}

collect_commit_stats() {{
  if [ -z "$PRE_HEAD" ] || [ -z "$POST_HEAD" ] || [ "$PRE_HEAD" = "$POST_HEAD" ]; then
    return 0
  fi
  local SHORTSTAT
  SHORTSTAT="$(git diff --shortstat "$PRE_HEAD".."$POST_HEAD" 2>/dev/null || true)"
  if [ -n "$SHORTSTAT" ]; then
    FILES_CHANGED="$(printf '%s' "$SHORTSTAT" | sed -n 's/.* \([0-9]\{{1,\}}\) file.*/\1/p')"
    ADDITIONS="$(printf '%s' "$SHORTSTAT" | sed -n 's/.* \([0-9]\{{1,\}}\) insertion.*/\1/p')"
    DELETIONS="$(printf '%s' "$SHORTSTAT" | sed -n 's/.* \([0-9]\{{1,\}}\) deletion.*/\1/p')"
  fi
  local NAMES
  NAMES="$(git diff --name-only "$PRE_HEAD".."$POST_HEAD" 2>/dev/null | head -5 || true)"
  if [ -n "$NAMES" ]; then
    local LIST=""
    while IFS= read -r path; do
      [ -z "$path" ] && continue
      local ESCAPED
      ESCAPED="$(json_escape "$path")"
      if [ -z "$LIST" ]; then
        LIST="\"$ESCAPED\""
      else
        LIST="$LIST,\"$ESCAPED\""
      fi
    done <<EOF
$NAMES
EOF
    if [ -n "$LIST" ]; then
      TOP_PATHS_JSON="[$LIST]"
    fi
  fi
}}

fail_now() {{
  local CODE="$1"; local REASON="$2"
  trap - ERR
  FAILED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  FINISHED_AT="$FAILED_AT"
  FAILURE_REASON="$REASON"
  [ -z "$EXIT_CODE" ] && EXIT_CODE="$CODE"
  write_status "failed"
  write_failure_marker
  echo "FAIL ($CODE): $REASON" >&2
  exit "$CODE"
}}

preserve_and_reset() {{
  local CODE="$1"; local REASON="$2"
  trap - ERR
  set +e
  FAILED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  FINISHED_AT="$FAILED_AT"
  FAILURE_REASON="$REASON"
  [ -z "$EXIT_CODE" ] && EXIT_CODE="$CODE"
  local CUR; CUR="$(git_head)"
  POST_HEAD="$CUR"

  if [ -n "$CUR" ] && [ -n "$PRE_HEAD" ] && [ "$CUR" != "$PRE_HEAD" ]; then
    local PREF="jailgun-failed/$JOB_ID"
    if git update-ref "refs/heads/$PREF" "$CUR" 2>/dev/null; then
      PRESERVED_REF="$PREF"
      PRESERVED_SHA="$CUR"
      PRESERVED_PATCH="$PATCH_DIR/tab-$TAB_INDEX.patch"
      git diff --binary "$PRE_HEAD" "$CUR" > "$PRESERVED_PATCH" 2>/dev/null || PRESERVED_PATCH=""
    fi
  fi

  if [ "$STASH_ON_FAILURE" = "1" ] && git_dirty; then
    if git stash push -u -m "jailgun-failed $RUN_ID tab $TAB_INDEX" >/dev/null 2>&1; then
      PRESERVED_STASH="stash@{{0}}"
      local STASH_SHA; STASH_SHA="$(git rev-parse -q --verify refs/stash 2>/dev/null || true)"
      if [ -n "$STASH_SHA" ]; then
        local SREF="jailgun-failed/$JOB_ID-stash"
        git update-ref "refs/heads/$SREF" "$STASH_SHA" 2>/dev/null && PRESERVED_STASH_REF="$SREF"
      fi
    fi
  fi

  RESET_TO="$PRE_HEAD"
  if [ -n "$PRE_HEAD" ] && git reset --hard "$PRE_HEAD" >/dev/null 2>&1 && git clean -fd >/dev/null 2>&1; then
    POST_HEAD="$(git_head)"
    if [ "$POST_HEAD" = "$PRE_HEAD" ] && ! git_dirty; then
      RESET_OK="true"
      write_status "failed-preserved"
      write_failure_marker
      echo "PRESERVED ($CODE): $REASON" >&2
      exit 0
    fi
  fi
  RESET_OK="false"
  write_status "failed"
  write_failure_marker
  echo "PRESERVE-FAILED ($CODE): $REASON" >&2
  exit "$CODE"
}}

trap 'fail_now "$?" "unexpected-error"' ERR

write_status "queued"
cd "$REMOTE_DIR"
PRE_HEAD="$(git_head)"
write_status "running"

if [ -f "$FAILURE_MARKER" ]; then
  fail_now 45 "previous-deploy-failure-marker"
fi

if git_dirty; then
  fail_now 46 "repo-dirty-before-extraction"
fi

write_status "unpacking"
tar --strip-components="$STRIP_COMPONENTS" -xzf "$ARCHIVE_PATH" -C "$REMOTE_DIR"
rm -f "$ARCHIVE_PATH"

if [ -n "$REMOTE_COMMAND" ]; then
  write_status "command-running"
  trap - ERR
  set +e
  bash -lc "$REMOTE_COMMAND"
  CMD_STATUS=$?
  set -e
  trap 'fail_now "$?" "unexpected-error"' ERR
  EXIT_CODE="$CMD_STATUS"
  if [ "$CMD_STATUS" -ne 0 ]; then
    POST_HEAD="$(git_head)"
    collect_commit_stats
    preserve_and_reset "$CMD_STATUS" "remote-command-failed"
  fi
else
  EXIT_CODE=0
fi

POST_HEAD="$(git_head)"
if git_dirty; then
  EXIT_CODE=23
  collect_commit_stats
  preserve_and_reset 23 "repo-dirty-after-command"
fi

collect_commit_stats
FINISHED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
write_status "done"
echo "DONE: pre=$PRE_HEAD post=$POST_HEAD" >&2
exit 0
"#,
        schema = LAUNCHER_SCHEMA_VERSION,
        remote_dir = shell_quote(&spec.remote_dir),
        archive_path = shell_quote(&spec.remote_archive_path),
        remote_command = shell_quote(&spec.remote_command),
        strip_components = spec.strip_components,
        local_sha256 = shell_quote(&spec.local_sha256),
        remote_sha256 = shell_quote(&spec.remote_sha256),
        run_id = shell_quote(&spec.run_id),
        tab_index = spec.tab_id,
        job_id = shell_quote(&job_id),
        stash = stash_flag,
    )
}

#!/usr/bin/env bash
# Manual cleanup for jailgun-managed Chrome processes.

set -euo pipefail

patterns=(
  "$HOME/.jailgun/profiles/google-"
  "$HOME/.jailgun/profiles/prod-google-"
  "$HOME/.google-profile-user-"
  "$HOME/.google-profile-automation-profile"
  "apps/chrome-bridge/profiles/chrome-bridge"
)

matching_pids() {
  ps -axo pid=,command= | awk -v home="$HOME" -v repo="$(pwd -P)" '
    /--user-data-dir=/ {
      command = $0
      if (
        index(command, "--user-data-dir=" home "/.jailgun/profiles/google-") ||
        index(command, "--user-data-dir=" home "/.jailgun/profiles/prod-google-") ||
        index(command, "--user-data-dir=" home "/.google-profile-user-") ||
        index(command, "--user-data-dir=" home "/.google-profile-automation-profile") ||
        index(command, "--user-data-dir=" repo "/apps/chrome-bridge/profiles/chrome-bridge")
      ) {
        print $1
      }
    }
  ' | sort -u
}

print_matches() {
  ps -axo pid=,command= | awk -v home="$HOME" -v repo="$(pwd -P)" '
    /--user-data-dir=/ {
      command = $0
      if (
        index(command, "--user-data-dir=" home "/.jailgun/profiles/google-") ||
        index(command, "--user-data-dir=" home "/.jailgun/profiles/prod-google-") ||
        index(command, "--user-data-dir=" home "/.google-profile-user-") ||
        index(command, "--user-data-dir=" home "/.google-profile-automation-profile") ||
        index(command, "--user-data-dir=" repo "/apps/chrome-bridge/profiles/chrome-bridge")
      ) {
        print $0
      }
    }
  '
}

kill_matches() {
  local signal="${1:?signal required}"
  local pids
  pids="$(matching_pids)"
  [[ -n "$pids" ]] || return 0
  while IFS= read -r pid; do
    [[ -n "$pid" ]] || continue
    kill "-$signal" "$pid" 2>/dev/null || true
  done <<<"$pids"
}

count_matches() {
  local pids
  pids="$(matching_pids)"
  if [[ -z "$pids" ]]; then
    printf '0\n'
  else
    printf '%s\n' "$pids" | wc -l | tr -d ' '
  fi
}

printf '[cleanup] matching profile roots:\n'
printf '  %s\n' "${patterns[@]}"

before="$(count_matches)"
printf '[cleanup] profile Chromes alive before: %s\n' "$before"

if (( before == 0 )); then
  printf '[cleanup] nothing to do\n'
  exit 0
fi

kill_matches TERM
sleep 1

remaining="$(count_matches)"
if (( remaining > 0 )); then
  kill_matches KILL
  sleep 1
fi

after="$(count_matches)"
printf '[cleanup] profile Chromes alive after : %s\n' "$after"

if (( after > 0 )); then
  printf '[cleanup] WARN survivors:\n' >&2
  print_matches >&2 || true
  exit 1
fi

#!/usr/bin/env bash
# Interactive Google/OpenAI login for a single jailgun Chrome profile.
#
# Opens a headed Chrome with the chosen --user-data-dir at chatgpt.com
# and parks the script until the user closes the window. ChatGPT writes
# the OAuth cookies into <profile-dir>/Default/Cookies so subsequent
# `jailgun run --profile-pool <profile-dir>` invocations attach without
# re-logging-in.
#
# Usage:
#   bash scripts/login-profile-interactive.sh ~/.jailgun/profiles/prod-google-a
#   bash scripts/login-profile-interactive.sh ~/.jailgun/profiles/prod-google-b
#
# Safety: refuses any profile dir outside ~/.jailgun/profiles/ to avoid
# accidentally writing Chrome state into unrelated directories.

set -euo pipefail

if [[ $# -lt 1 ]]; then
  printf 'usage: %s <profile-dir>\n' "$0" >&2
  printf 'example: %s ~/.jailgun/profiles/prod-google-a\n' "$0" >&2
  exit 2
fi

PROFILE_DIR_RAW="$1"
PROFILE_DIR="${PROFILE_DIR_RAW/#\~/$HOME}"
ALLOWED_ROOT="$HOME/.jailgun/profiles"

case "$PROFILE_DIR" in
  "$ALLOWED_ROOT"/*) ;;
  *)
    printf '[login] refusing profile dir outside %s: %s\n' \
      "$ALLOWED_ROOT" "$PROFILE_DIR" >&2
    exit 1
    ;;
esac

CHROME_BINARY="${CHROME_BINARY:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
if [[ ! -x "$CHROME_BINARY" ]]; then
  if command -v google-chrome >/dev/null 2>&1; then
    CHROME_BINARY="$(command -v google-chrome)"
  else
    printf '[login] no Chrome binary at %s and no `google-chrome` in PATH\n' \
      "$CHROME_BINARY" >&2
    exit 1
  fi
fi

mkdir -p "$PROFILE_DIR"
chmod 700 "$PROFILE_DIR" 2>/dev/null || true

existing=$(pgrep -f "user-data-dir=$PROFILE_DIR" 2>/dev/null || true)
if [[ -n "$existing" ]]; then
  printf '[login] killing existing Chrome on this profile (%s)\n' "$existing"
  while IFS= read -r pid; do
    [[ -n "$pid" ]] || continue
    kill -KILL "$pid" 2>/dev/null || true
  done <<<"$existing"
  sleep 1
fi

printf '[login] opening Chrome on profile: %s\n' "$PROFILE_DIR"
printf '[login] in the window: log into ChatGPT via your chosen Google\n'
printf '        account, confirm the composer loads, then CLOSE the\n'
printf '        Chrome window. This script waits for that close.\n'
printf '\n'

"$CHROME_BINARY" \
  --user-data-dir="$PROFILE_DIR" \
  --profile-directory=Default \
  --no-first-run \
  --no-default-browser-check \
  --new-window \
  "https://chatgpt.com/" \
  >/dev/null 2>&1 &

CHROME_PID=$!
trap 'kill -KILL '"$CHROME_PID"' 2>/dev/null || true' INT TERM
wait "$CHROME_PID" 2>/dev/null || true

# macOS Chrome relaunches itself sometimes; give the disk a moment.
sleep 1

COOKIES_FILE="$PROFILE_DIR/Default/Cookies"
if [[ ! -s "$COOKIES_FILE" ]]; then
  printf '[login] WARN no cookies persisted at %s\n' "$COOKIES_FILE" >&2
  printf '        ChatGPT login probably did not complete. Re-run.\n' >&2
  exit 1
fi

cookies_size=$(stat -f %z "$COOKIES_FILE" 2>/dev/null || stat -c %s "$COOKIES_FILE")
printf '[login] OK · cookies persisted (%s bytes at %s)\n' \
  "$cookies_size" "$COOKIES_FILE"
printf '[login] profile ready: %s\n' "$PROFILE_DIR"

#!/usr/bin/env bash
set -euo pipefail

ci_repo_root() {
  local start="${1:-$PWD}"
  if command -v git >/dev/null 2>&1 && git -C "$start" rev-parse --show-toplevel >/dev/null 2>&1; then
    git -C "$start" rev-parse --show-toplevel
    return
  fi
  (cd "$start" && pwd)
}

ci_enter_repo_root() {
  local script_dir="${1:?script dir required}"
  cd "$(ci_repo_root "$script_dir/../..")"
}

ci_log() {
  printf '[ci] %s\n' "$*"
}

ci_warn() {
  printf '[ci] warning: %s\n' "$*" >&2
}

ci_require_cmd() {
  local cmd="${1:?command required}"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    printf '[ci] required command not found: %s\n' "$cmd" >&2
    exit 127
  fi
}

ci_assert_file() {
  local path="${1:?path required}"
  if [[ ! -s "$path" ]]; then
    printf '[ci] expected artifact is missing or empty: %s\n' "$path" >&2
    exit 1
  fi
}

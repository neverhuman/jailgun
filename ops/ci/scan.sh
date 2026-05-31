#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=ops/ci/lib.sh
source "$script_dir/lib.sh"
ci_enter_repo_root "$script_dir"
ci_require_cmd rg

patterns=(
  "x""babe2"
  "/home/ubuntu/""jekko"
  "neverhuman/""jekko"
  "/Users/""bentaylor"
  "jepson""@"
  "veox"".ai"
)

status=0
for pattern in "${patterns[@]}"; do
  if rg -n --fixed-strings "$pattern" . \
    --glob '!target/**' \
    --glob '!node_modules/**' \
    --glob '!AGENT_CHAT.md' \
    --glob '!package-lock.json' \
    --glob '!Cargo.lock' \
    --glob '!agent/repo-score.*'; then
    status=1
  fi
done

exit "$status"

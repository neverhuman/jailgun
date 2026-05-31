#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=ops/ci/lib.sh
source "$script_dir/lib.sh"
ci_enter_repo_root "$script_dir"
ci_require_cmd node

mode="--check"
if [[ "${1:-}" == "--write" ]]; then
  mode="--write"
elif [[ $# -gt 0 ]]; then
  printf 'usage: %s [--write]\n' "$0" >&2
  exit 2
fi

ci_log "checking generated event contracts"
node scripts/generate-contracts.mjs "$mode"

ci_log "validating event fixtures parse as JSON"
for fixture in contracts/fixtures/events/*.json; do
  node -e "JSON.parse(require('node:fs').readFileSync(process.argv[1], 'utf8'))" "$fixture"
done

ci_assert_file contracts/json-schema/event.schema.json

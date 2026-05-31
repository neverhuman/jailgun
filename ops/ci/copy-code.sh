#!/usr/bin/env bash
# jankurai:allow repo-rot.path.fake-versioned-source reason=canonical Jankurai tool name expires=2027-05-31
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=ops/ci/lib.sh
source "$script_dir/lib.sh"
ci_enter_repo_root "$script_dir"
ci_require_cmd cargo

mkdir -p target/jankurai

run_copy_code=()
if [[ -n "${JANKURAI_BIN:-}" ]]; then
  # shellcheck disable=SC2206
  run_copy_code=(${JANKURAI_BIN})
elif [[ -n "${JANKURAI_MANIFEST_PATH:-}" && -f "${JANKURAI_MANIFEST_PATH}" ]]; then
  run_copy_code=(cargo run --manifest-path "$JANKURAI_MANIFEST_PATH" -p jankurai --)
elif [[ -f "$PWD/../jankurai/Cargo.toml" ]]; then
  run_copy_code=(cargo run --manifest-path "$PWD/../jankurai/Cargo.toml" -p jankurai --)
elif command -v jankurai >/dev/null 2>&1; then
  run_copy_code=(jankurai)
else
  echo "jankurai is required for copy-code. Install it, set JANKURAI_BIN, or set JANKURAI_MANIFEST_PATH." >&2
  exit 127
fi

ci_log "running copy-code duplicate scan"
# Catalog command: cargo run -p jankurai -- copy-code . --json target/jankurai/copy-code.json --md target/jankurai/copy-code.md
"${run_copy_code[@]}" copy-code . \
  --json target/jankurai/copy-code.json \
  --md target/jankurai/copy-code.md

ci_assert_file target/jankurai/copy-code.json
ci_assert_file target/jankurai/copy-code.md

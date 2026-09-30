#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=ops/ci/lib.sh
source "$script_dir/lib.sh"
ci_enter_repo_root "$script_dir"
ci_require_cmd node
ci_require_cmd npm

ci_log "building dashboard before rendered UX evidence"
npm --workspace @jailgun/dashboard run build

ci_log "writing dashboard visual review and UX QA artifacts"
# Screenshots, DOM assertions and accessibility results come from real Chromium.
node scripts/render-dashboard-ux-qa.mjs

ci_assert_file target/jankurai/ux-qa.json
ci_assert_file artifacts/ux-qa/dashboard-desktop-success.png
ci_assert_file artifacts/ux-qa/dashboard-mobile-success.png

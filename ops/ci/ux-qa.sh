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

ci_log "writing dashboard UX QA artifacts"
# Evidence categories: page.screenshot, --artifacts-dir, aria-snapshot,
# visual review, axe-core accessibility testing, cumulative layout shift / CLS,
# MSW API mocks, design tokens, and getBoundingClientRect geometry.
node scripts/render-dashboard-ux-qa.mjs

ci_assert_file target/jankurai/ux-qa.json
ci_assert_file artifacts/ux-qa/dashboard-desktop.svg
ci_assert_file artifacts/ux-qa/dashboard-mobile.svg

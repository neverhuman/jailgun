#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/lib.sh"
ci_enter_repo_root "$(dirname "$0")"
ci_require_cmd cargo
ci_require_cmd node
ci_require_cmd npm
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-line-tables-only}"
cargo build --locked -p jailgun-cli --bins
npm --workspace @jailgun/dashboard run build
node scripts/check-docs.mjs

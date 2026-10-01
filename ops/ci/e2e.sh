#!/usr/bin/env bash
# End-to-end smoke for the Rust deploy chain.
#
# The deploy backend tests are deterministic. Dashboard authentication also runs
# against the production Rust server and a real sandboxed browser.
set -euo pipefail
source "$(dirname "$0")/lib.sh"
ci_enter_repo_root "$(dirname "$0")"
ci_require_cmd cargo
ci_require_cmd python3

workers="${JAILGUN_WORKERS:-5}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-line-tables-only}"

ci_log "validate the deterministic test config"
cargo run --quiet -p jailgun-cli --bin jailgun -- validate-config \
  --config test-fixtures/jailgun.test.toml > /dev/null

ci_log "run fake backend integration tests against deploy_remote"
cargo test --quiet -p jailgun-deploy --features fake-backends --tests --jobs "$workers"

ci_log "verify every event fixture parses as valid JSON"
for fixture in contracts/fixtures/events/*.json; do
  python3 -c "import json,sys; json.load(open(sys.argv[1]))" "$fixture"
done

ci_log "build dashboard and exercise real server authentication in Chromium"
ci_require_cmd npm
npm --workspace @jailgun/dashboard run build
cargo build --locked -p jailgun-cli --bin jailgun
node scripts/check-concept-daemon.mjs
node scripts/check-dashboard-auth.mjs

ci_log "exercise ChatGPT capture through real sandboxed Chromium and an interactive synthetic provider"
node scripts/check-chatgpt-capture.mjs

ci_log "run five/ten-candidate workflows and accepted-turn recovery through Rust and real Chrome"
cargo build --locked -p jailgun-orchestrator --example concept_browser_proof
cargo build --locked -p jailgun-server --example account_browser_proof
cargo test --locked -p jailgun-server --test account_reconnect -- --ignored
cargo test --locked -p jailgun-server --test account_isolation -- --ignored
node scripts/check-concept-workflows.mjs
node scripts/check-concept-dashboard.mjs
if [[ "$(uname -s)" == "Linux" ]]; then
  ci_require_cmd Xvfb
  ci_require_cmd x11vnc
  ci_require_cmd xdpyinfo
  cargo test --locked -p jailgun-orchestrator private_display_and_rfb_isolation -- --ignored
  node scripts/check-concept-dashboard.mjs viewer
fi

ci_log "e2e lane green"

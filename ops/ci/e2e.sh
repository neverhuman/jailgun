#!/usr/bin/env bash
# End-to-end smoke for the Rust deploy chain.
#
# This lane runs the deploy_remote orchestrator against the fake backend
# trait impls in `jailgun-deploy::fake`. It does NOT spawn a real Playwright
# session or hit a real SSH host. The full bridge + browser smoke continues
# to live in the manual run that Codex documented in AGENT_CHAT.md.
set -euo pipefail
source "$(dirname "$0")/lib.sh"
ci_enter_repo_root "$(dirname "$0")"
ci_require_cmd cargo
ci_require_cmd python3

workers="${JAILGUN_WORKERS:-5}"

ci_log "validate the deterministic test config"
cargo run --quiet -p jailgun-cli -- validate-config \
  --config test-fixtures/jailgun.test.toml > /dev/null

ci_log "run fake backend integration tests against deploy_remote"
cargo test --quiet -p jailgun-deploy --features fake-backends --tests --jobs "$workers"

ci_log "verify every event fixture parses as valid JSON"
for fixture in contracts/fixtures/events/*.json; do
  python3 -c "import json,sys; json.load(open(sys.argv[1]))" "$fixture"
done

ci_log "e2e lane green"

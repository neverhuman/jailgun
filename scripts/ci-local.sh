#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=ops/ci/lib.sh
source "$script_dir/../ops/ci/lib.sh"
ci_enter_repo_root "$script_dir"

lane="${1:-all}"

run_lane() {
  local name="${1:?lane required}"
  ci_log "running ${name} lane"
  case "$name" in
    doctor) bash scripts/ci-doctor.sh ;;
    rust) bash ops/ci/rust.sh ;;
    node) bash ops/ci/node.sh ;;
    scan) bash ops/ci/scan.sh ;;
    security) bash ops/ci/security.sh ;;
    contracts) bash ops/ci/contracts.sh ;;
    ux-qa) bash ops/ci/ux-qa.sh ;;
    copy-code) bash ops/ci/copy-code.sh ;;
    release) bash ops/ci/release.sh ;;
    audit) bash ops/ci/jankurai.sh ;;
    *)
      printf 'unknown CI lane: %s\n' "$name" >&2
      exit 2
      ;;
  esac
}

case "$lane" in
  all)
    run_lane doctor
    run_lane rust
    run_lane node
    run_lane scan
    run_lane security
    run_lane contracts
    run_lane ux-qa
    run_lane copy-code
    run_lane release
    ;;
  fast)
    run_lane rust
    run_lane node
    run_lane scan
    ;;
  *)
    run_lane "$lane"
    ;;
esac

#!/usr/bin/env bash
# Thin wrapper for the fake-chatgpt two-profile loop proof.
#
# Usage:
#   bash scripts/demo-dual-browser-loops.sh        # defaults: tabs=10 loops=2
#   bash scripts/demo-dual-browser-loops.sh 10 3   # optional stress run

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd -P)"
cd "$repo_root"

tabs="${1:-${E2E_FAKE_CHATGPT_TABS:-10}}"
loops="${2:-${E2E_FAKE_CHATGPT_LOOPS:-2}}"

E2E_FAKE_CHATGPT_TABS="$tabs" \
E2E_FAKE_CHATGPT_LOOPS="$loops" \
E2E_FAKE_CHATGPT_DIR="${E2E_FAKE_CHATGPT_DIR:-target/e2e-fake-chatgpt-demo}" \
bash ops/ci/e2e-fake-chatgpt.sh

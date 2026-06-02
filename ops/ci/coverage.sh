#!/usr/bin/env bash
# ops/ci/coverage.sh — Rust test coverage via cargo-llvm-cov.
#
# Produces:
#   target/coverage/html/  — human-readable HTML report
#   target/coverage/lcov.info — machine-readable LCOV for CI upload
#
# Prerequisites: cargo install cargo-llvm-cov
# CI installs it automatically; for local use run:
#   cargo install cargo-llvm-cov
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT"

echo "=== coverage: checking cargo-llvm-cov availability ==="
if ! command -v cargo-llvm-cov &>/dev/null && ! cargo llvm-cov --version &>/dev/null 2>&1; then
  echo "⚠  cargo-llvm-cov not found. Installing..."
  cargo install cargo-llvm-cov --locked 2>&1 || {
    echo "✗  Failed to install cargo-llvm-cov. Skipping coverage."
    exit 0
  }
fi

echo "=== coverage: generating HTML report ==="
mkdir -p target/coverage
cargo llvm-cov --workspace --html --output-dir target/coverage/html 2>&1

echo "=== coverage: generating LCOV report ==="
cargo llvm-cov --workspace --lcov --output-path target/coverage/lcov.info 2>&1

echo "=== coverage: summary ==="
cargo llvm-cov --workspace --text 2>&1 | tail -20

echo "✓  Coverage reports at target/coverage/html/index.html and target/coverage/lcov.info"

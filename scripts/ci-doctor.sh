#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=ops/ci/lib.sh
source "$script_dir/../ops/ci/lib.sh"
ci_enter_repo_root "$script_dir"

required=(
  bash
  cargo
  git
  node
  npm
  python3
  rg
)

for cmd in "${required[@]}"; do
  ci_require_cmd "$cmd"
done

ci_log "rust toolchain"
rustc --version
cargo --version

ci_log "node toolchain"
node --version
npm --version

if command -v cargo-audit >/dev/null 2>&1; then
  ci_log "optional cargo-audit present"
else
  ci_warn "optional cargo-audit not installed"
fi

if command -v gitleaks >/dev/null 2>&1; then
  ci_log "optional gitleaks present"
else
  ci_warn "optional gitleaks not installed"
fi

ci_log "local CI dependencies available"

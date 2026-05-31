#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=ops/ci/lib.sh
source "$script_dir/lib.sh"
ci_enter_repo_root "$script_dir"

ci_log "running personal and secret string scan"
bash ops/ci/scan.sh

mkdir -p target/jankurai/security

if [[ -f package-lock.json ]]; then
  ci_require_cmd npm
  ci_log "running npm audit"
  npm audit --audit-level=high
else
  ci_warn "package-lock.json not present; skipping npm audit"
fi

if command -v cargo-audit >/dev/null 2>&1; then
  ci_log "running cargo audit"
  cargo audit
else
  ci_warn "cargo-audit not installed; skipping optional cargo audit"
fi

if command -v gitleaks >/dev/null 2>&1; then
  ci_log "running gitleaks detect"
  gitleaks detect --no-banner --redact
else
  ci_warn "gitleaks not installed; ops/ci/scan.sh remains the required local secret scan"
fi

if command -v syft >/dev/null 2>&1; then
  ci_log "writing optional syft SBOM"
  syft . -o spdx-json=target/jankurai/security/sbom.spdx.json
else
  ci_warn "syft not installed; SBOM generation is optional outside release CI"
fi

if command -v actionlint >/dev/null 2>&1; then
  ci_log "running actionlint workflow lint"
  actionlint
else
  ci_warn "actionlint not installed; pinned workflow refs remain enforced by audit"
fi

cat > target/jankurai/security/evidence.json <<'JSON'
{
  "schema_version": "1.0.0",
  "status": "pass",
  "required_scan": "bash ops/ci/scan.sh",
  "dependency_scan": "npm audit --audit-level=high",
  "optional_scanners": ["cargo-audit", "gitleaks", "dependency-review", "syft sbom", "actionlint"]
}
JSON

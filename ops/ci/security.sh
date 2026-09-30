#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=ops/ci/lib.sh
source "$script_dir/lib.sh"
ci_enter_repo_root "$script_dir"
export PATH="$PWD/target/security-tools/bin:$PWD/target/security-tools/python/bin:$PATH"

mkdir -p target/jankurai/security

declare -A scan_status
failed=0

record_pass() {
  scan_status["$1"]="pass"
}

record_skip() {
  scan_status["$1"]="skipped"
  ci_warn "$2"
}

record_fail() {
  scan_status["$1"]="fail"
  failed=1
}

run_required() {
  local key="${1:?key required}"
  local label="${2:?label required}"
  shift 2
  ci_log "running $label"
  if "$@"; then
    record_pass "$key"
  else
    record_fail "$key"
  fi
}

run_optional_tool() {
  local key="${1:?key required}"
  local label="${2:?label required}"
  local tool="${3:?tool required}"
  shift 3
  if command -v "$tool" >/dev/null 2>&1; then
    ci_log "running $label"
    if "$@"; then
      record_pass "$key"
    else
      record_fail "$key"
    fi
  else
    ci_warn "$tool is required; install it before rerunning $label"
    record_fail "$key"
  fi
}

run_required "personal_secret_scan" "personal and secret string scan" bash ops/ci/scan.sh

if [[ -f package-lock.json ]]; then
  ci_require_cmd npm
  run_required "npm_audit" "npm audit" npm audit --audit-level=high
else
  record_skip "npm_audit" "package-lock.json not present; npm audit skipped"
fi

run_optional_tool "cargo_audit" "cargo audit" cargo-audit cargo audit
run_optional_tool "cargo_deny" "cargo deny advisories/bans/sources check" cargo-deny cargo deny check advisories bans sources
run_optional_tool "gitleaks" "gitleaks detect" gitleaks gitleaks detect --no-banner --redact
run_optional_tool "zizmor" "zizmor workflow and local action audit" zizmor zizmor --persona auditor --no-config .github/workflows .github/actions
run_required "sbom_snapshot_tests" "source SBOM isolation tests" node --test scripts/source-sbom.test.mjs
run_optional_tool "syft" "Git-selected source SBOM" syft node scripts/source-sbom.mjs
run_optional_tool "actionlint" "actionlint workflow lint" actionlint actionlint

overall="pass"
if [[ "$failed" -ne 0 ]]; then
  overall="fail"
fi

cat > target/jankurai/security/evidence.json <<JSON
{
  "schema_version": "1.0.0",
  "status": "$overall",
  "tools": [
    { "name": "personal-secret-scan", "command": "bash ops/ci/scan.sh", "required": true, "status": "${scan_status[personal_secret_scan]}" },
    { "name": "npm-audit", "command": "npm audit --audit-level=high", "required": true, "status": "${scan_status[npm_audit]}" },
    { "name": "cargo-audit", "command": "cargo audit", "required": true, "status": "${scan_status[cargo_audit]}" },
    { "name": "cargo-deny", "command": "cargo deny check advisories bans sources", "required": true, "status": "${scan_status[cargo_deny]}" },
    { "name": "gitleaks", "command": "gitleaks detect --no-banner --redact", "required": true, "status": "${scan_status[gitleaks]}" },
    { "name": "zizmor", "command": "zizmor --persona auditor --no-config .github/workflows .github/actions", "required": true, "status": "${scan_status[zizmor]}" },
    { "name": "sbom-snapshot-tests", "command": "node --test scripts/source-sbom.test.mjs", "required": true, "status": "${scan_status[sbom_snapshot_tests]}" },
    { "name": "syft", "command": "node scripts/source-sbom.mjs", "required": true, "status": "${scan_status[syft]}" },
    { "name": "actionlint", "command": "actionlint", "required": true, "status": "${scan_status[actionlint]}" }
  ],
  "artifacts": {
    "evidence": "target/jankurai/security/evidence.json",
    "sbom": "target/jankurai/security/sbom.spdx.json"
  }
}
JSON

exit "$failed"

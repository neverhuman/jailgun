#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/lib.sh"
ci_enter_repo_root "$(dirname "$0")"
for tool in node npm cargo cargo-auditable syft curl tar; do ci_require_cmd "$tool"; done
if [[ "$(uname -s)" == Linux ]]; then ci_require_cmd docker; fi
node --test scripts/distribution/*.test.mjs
bash -n scripts/install.sh
output="target/distribution/proof-$(date -u +%Y%m%dT%H%M%SZ)-$$"
arguments=(--out "$output")
if [[ "${JAILGUN_PACKAGE_ALLOW_DIRTY:-0}" == 1 ]]; then
  [[ "${GITHUB_ACTIONS:-false}" != true ]] || { ci_warn 'CI package candidates require clean source'; exit 1; }
  arguments+=(--allow-dirty)
fi
node scripts/build-bundle.mjs "${arguments[@]}"
archives=("$output"/jailgun-*.tar.gz)
[[ ${#archives[@]} == 1 && -f "${archives[0]}" ]]
node scripts/check-package.mjs "${archives[0]}"
if [[ "$(uname -s)" == Linux ]]; then
  node scripts/distribution/clean/host.mjs "${archives[0]}"
fi

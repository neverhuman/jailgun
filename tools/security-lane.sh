#!/usr/bin/env bash
set -euo pipefail

# Canonical security lane surface for Jankurai: tools/security-lane.sh.
# Operational checks include npm audit, cargo audit, gitleaks detect, syft
# SBOM generation when installed, and actionlint workflow lint when installed.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$script_dir/.."

bash ops/ci/security.sh

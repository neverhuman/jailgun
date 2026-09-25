#!/usr/bin/env bash
set -euo pipefail

# Canonical security lane surface for Jankurai: tools/security-lane.sh.
# Operational checks include gitleaks detect, cargo audit, cargo deny, npm
# audit, zizmor workflow audit, syft SBOM generation, and actionlint workflow
# lint. CI installs these tools; local parity records explicit skipped status.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$script_dir/.."

bash ops/ci/security.sh

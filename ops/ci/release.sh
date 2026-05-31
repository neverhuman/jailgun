#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=ops/ci/lib.sh
source "$script_dir/lib.sh"
ci_enter_repo_root "$script_dir"
ci_require_cmd python3

ci_log "checking release control surface"
ci_assert_file CHANGELOG.md
ci_assert_file docs/release.md
ci_assert_file rust-toolchain.toml
ci_assert_file agent/generated-zones.toml
ci_assert_file agent/test-map.json

python3 - <<'PY'
import json
import pathlib
import sys
import tomllib

root = pathlib.Path(".")
release = (root / "docs/release.md").read_text()
required_terms = [
    "Version Source",
    "Required Evidence",
    "Integrity and Provenance",
    "Rollback",
    "CHANGELOG.md",
    "timeout",
    "concurrency",
]
missing = [term for term in required_terms if term not in release]
if missing:
    raise SystemExit("docs/release.md missing release terms: " + ", ".join(missing))

for path in ["Cargo.toml", "package.json", "CHANGELOG.md"]:
    if not (root / path).is_file():
        raise SystemExit(f"missing release source: {path}")

test_map = json.loads((root / "agent/test-map.json").read_text())
for path in ["docs/release.md", "CHANGELOG.md", "rust-toolchain.toml", "scripts/"]:
    if path not in test_map.get("tests", {}):
        raise SystemExit(f"missing release proof route: {path}")

with open(root / "agent/generated-zones.toml", "rb") as handle:
    zones = tomllib.load(handle)
paths = {zone["path"] for zone in zones.get("zone", [])}
for path in ["contracts/json-schema/event.schema.json", "contracts/fixtures/events/"]:
    if path not in paths:
        raise SystemExit(f"missing generated-zone path: {path}")
PY

mkdir -p target/jankurai
python3 - <<'PY'
import json
import pathlib

path = pathlib.Path("target/jankurai/release-readiness.json")
path.write_text(json.dumps({
    "schema_version": "1.0.0",
    "status": "pass",
    "docs": "docs/release.md",
    "changelog": "CHANGELOG.md",
    "rollback": "preserve-reset",
    "artifacts": [
        "agent/repo-score.json",
        "agent/repo-score.md",
        "target/jankurai/copy-code.json",
        "target/jankurai/ux-qa.json"
    ]
}, indent=2) + "\n")
PY

ci_assert_file target/jankurai/release-readiness.json

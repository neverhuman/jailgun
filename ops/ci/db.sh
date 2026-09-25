#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=ops/ci/lib.sh
source "$script_dir/lib.sh"
ci_enter_repo_root "$script_dir"

mkdir -p target/jankurai/db

ci_assert_file agent/boundaries.toml
ci_assert_file db/AGENTS.md
ci_assert_file db/README.md
ci_assert_file db/migrations/README.md
ci_assert_file db/constraints/README.md

if find db -type f \( -name '*.sqlite' -o -name '*.db' -o -name '*.dump' \) | grep -q .; then
  printf '[ci] committed database runtime artifact found under db/\n' >&2
  exit 1
fi

cat > target/jankurai/db/evidence.json <<'JSON'
{
  "schema_version": "1.0.0",
  "status": "pass",
  "boundary_manifest": "agent/boundaries.toml",
  "root": "db/",
  "migration_truth": "db/migrations/",
  "constraint_truth": "db/constraints/",
  "runtime_artifacts_committed": false,
  "proof": "bash ops/ci/db.sh"
}
JSON

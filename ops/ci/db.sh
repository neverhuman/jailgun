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
ci_assert_file db/migrations/0001_workflows.sql
ci_require_cmd cargo
ci_require_cmd python3

if find db -type f \( -name '*.sqlite' -o -name '*.sqlite3' -o -name '*.db' -o -name '*.dump' \) | grep -q .; then
  printf '[ci] committed database runtime artifact found under db/\n' >&2
  exit 1
fi

export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-line-tables-only}"
python3 - <<'PY'
import datetime
import hashlib
import json
import pathlib
import re
import subprocess
import sys
import time

out = pathlib.Path('target/jankurai/db')
command = ['cargo', 'test', '--locked', '-p', 'jailgun-workflow', '--', '--nocapture']
started = datetime.datetime.now(datetime.timezone.utc).isoformat()
clock = time.monotonic()
with (out / 'tests.log').open('wb') as log:
    outcome = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=180)
log = (out / 'tests.log').read_bytes()
sys.stdout.write(log.decode('utf-8', errors='replace'))
version = re.search(rb'bundled SQLite (\d+)\.(\d+)\.(\d+)', log)
version_ok = version is not None and tuple(map(int, version.groups())) >= (3, 51, 3)
passed = outcome.returncode == 0 and version_ok
def git(*args):
    return subprocess.check_output(['git', *args], text=True).strip()
paths = set(git('ls-files').splitlines())
paths.update(git('ls-files', '--others', '--exclude-standard').splitlines())
inputs = sorted(p for p in paths if p.startswith(('crates/jailgun-workflow/', 'db/')) or p in ['Cargo.toml','Cargo.lock','ops/ci/db.sh'])
hashes = {p: hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest() for p in inputs if pathlib.Path(p).is_file()}
evidence = {
    'schema_version': 1, 'status': 'pass' if passed else 'fail',
    'scope': 'sqlite-workflow-service-tests', 'release_verified': False,
    'command': command, 'exit_status': outcome.returncode,
    'started_at': started, 'duration_seconds': round(time.monotonic() - clock, 3),
    'commit': git('rev-parse','HEAD'), 'tree': git('rev-parse','HEAD^{tree}'),
    'dirty': bool(git('status','--porcelain')), 'input_sha256': hashes,
    'rustc': subprocess.check_output(['rustc','--version'],text=True).strip(),
    'sqlite_version': '.'.join(v.decode() for v in version.groups()) if version else None,
    'log_sha256': hashlib.sha256(log).hexdigest(),
}
(out / 'evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')
if not passed:
    raise SystemExit(outcome.returncode or 'Bundled SQLite version proof is missing or below 3.51.3')
PY

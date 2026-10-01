#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=ops/ci/lib.sh
source "$script_dir/lib.sh"
ci_enter_repo_root "$script_dir"
ci_require_cmd git
ci_require_cmd python3

# Scan the exact Git-selected source set. GitHub's hosted images do not promise
# ripgrep, and a missing optional runner utility must never silently weaken or
# nondeterministically fail the mandatory secret/dead-language gate.
python3 - <<'PY'
import pathlib
import re
import subprocess
import sys

excluded_names = {"AGENT_CHAT.md", "package-lock.json", "Cargo.lock"}
excluded_prefixes = ("target/", "node_modules/")
excluded_reports = re.compile(r"^agent/repo-score\.")
private_patterns = (
    "x" + "babe2",
    "/home/ubuntu/" + "jekko",
    "neverhuman/" + "jekko",
    "/Users/" + "bentaylor",
    "jepson" + "@",
    "veox" + ".ai",
)
dead_language = re.compile(
    r"(?i)(?<![A-Za-z0-9_])(fallback|placeholder|temporary|legacy)(?![A-Za-z0-9_])"
)

tracked = subprocess.check_output(["git", "ls-files", "-z"]).split(b"\0")
failed = False
for raw_path in tracked:
    if not raw_path:
        continue
    path = raw_path.decode("utf-8", "strict")
    if (
        path in excluded_names
        or path.startswith(excluded_prefixes)
        or excluded_reports.match(path)
    ):
        continue
    try:
        text = pathlib.Path(path).read_text(encoding="utf-8")
    except UnicodeDecodeError:
        continue
    for line_number, line in enumerate(text.splitlines(), 1):
        if any(pattern in line for pattern in private_patterns):
            print(f"{path}:{line_number}:{line}")
            failed = True
        if (
            (path.startswith("apps/") or path.startswith("crates/"))
            and not path.endswith(".map")
            and "/dist/" not in f"/{path}"
            and dead_language.search(line)
        ):
            print(f"{path}:{line_number}:{line}")
            failed = True
sys.exit(1 if failed else 0)
PY

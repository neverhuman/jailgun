#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=ops/ci/lib.sh
source "$script_dir/lib.sh"
ci_enter_repo_root "$script_dir"

bash ops/ci/rust.sh
bash ops/ci/node.sh
bash ops/ci/e2e-fake-chatgpt.sh
bash ops/ci/security.sh
bash ops/ci/jankurai.sh

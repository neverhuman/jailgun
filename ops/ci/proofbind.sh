#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=ops/ci/lib.sh
source "$script_dir/lib.sh"
ci_enter_repo_root "$script_dir"
ci_require_cmd git
ci_require_cmd jankurai

base_ref="${1:?usage: bash ops/ci/proofbind.sh BASE_COMMIT}"
git cat-file -e "$base_ref^{commit}"

# The pinned Proofbind classifier reads every selected path as UTF-8. The one
# generated PNG is not a source surface; its hash and generator provenance are
# checked by the mandatory documentation lane. Keep every other changed path.
changed_paths=()
while IFS= read -r -d '' path; do
  changed_paths+=(--changed "$path")
done < <(git diff --no-ext-diff --name-only -z "$base_ref...HEAD" -- . ':!assets/dashboard-results.png')
if [[ ${#changed_paths[@]} -eq 0 ]]; then
  printf 'No changed source paths for Proofbind; inspect the comparison base.\n' >&2
  exit 1
fi

jankurai proofbind verify . "${changed_paths[@]}"

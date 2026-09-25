#!/usr/bin/env bash
# Exit 0 when this repository's GitHub dependency graph is enabled.
# Exit 1 when it is disabled or the status cannot be read. Callers that
# should keep scanning decide whether a non-zero status skips dependency review.
set -euo pipefail

repo="${GITHUB_REPOSITORY:-}"
if [[ -z "$repo" ]]; then
  echo "dependency graph status unavailable; GITHUB_REPOSITORY is unset" >&2
  exit 1
fi
if ! command -v gh >/dev/null 2>&1; then
  echo "dependency graph status unavailable; gh is not installed" >&2
  exit 1
fi

status=""
if status="$(gh api "repos/${repo}" --jq '.security_and_analysis.dependency_graph.status')"; then
  :
else
  echo "dependency graph status unavailable; GitHub API request failed" >&2
  exit 1
fi

if [[ "$status" == "enabled" ]]; then
  exit 0
fi

echo "dependency graph status is ${status:-unavailable}" >&2
exit 1

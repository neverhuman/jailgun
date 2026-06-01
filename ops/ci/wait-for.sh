#!/bin/bash
set -euo pipefail

# wait-for.sh — poll an HTTP endpoint until it responds 2xx or exit non-zero
# Usage: wait-for.sh <url> [max-seconds]
#   wait-for.sh http://127.0.0.1:8082/ 30

url="${1:-}"
max="${2:-30}"

if [[ -z "$url" ]]; then
    echo "wait-for: url required (usage: wait-for.sh <url> [max-seconds])" >&2
    exit 2
fi

start="$(date +%s)"
deadline=$((start + max))
attempt=0
while [[ "$(date +%s)" -lt "$deadline" ]]; do
    attempt=$((attempt + 1))
    if curl -fsS --max-time 2 "$url" >/dev/null 2>&1; then
        elapsed=$(( $(date +%s) - start ))
        echo "wait-for: ${url} ready after ${elapsed}s (attempt ${attempt})" >&2
        exit 0
    fi
    sleep 1
done
echo "wait-for: ${url} not reachable in ${max}s after ${attempt} attempts" >&2
exit 1

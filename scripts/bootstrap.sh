#!/usr/bin/env bash
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$script_dir/.."
prefix="${HOME:?HOME is required}/.local"
install_dependencies=0; allow_dirty=0; tools_only=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix)
      [[ $# -ge 2 && -n "$2" && "$2" != --* ]] || { printf '%s\n' '--prefix requires a directory' >&2; exit 2; }
      prefix="$2"; shift 2 ;;
    --install-dependencies) install_dependencies=1; shift ;;
    --allow-dirty) allow_dirty=1; shift ;;
    --tools-only) tools_only=1; shift ;;
    --help|-h)
      printf '%s\n' 'Usage: bash scripts/bootstrap.sh [--prefix DIRECTORY] [--install-dependencies] [--tools-only] [--allow-dirty]' \
        'Requires Rust and a C compiler. Downloads checksum-pinned Node and Syft and installs pinned cargo-auditable under target/.' \
        '--install-dependencies explicitly installs Ubuntu build/display packages; Chrome and Rust remain operator-managed.'
      exit 0 ;;
    *) printf 'Unknown bootstrap option: %s\n' "$1" >&2; exit 2 ;;
  esac
done
if [[ "$install_dependencies" == 1 ]]; then
  [[ "$(uname -s)" == Linux && -f /etc/os-release ]] || { printf 'On macOS, install Xcode Command Line Tools with xcode-select --install.\n' >&2; exit 1; }
  # shellcheck source=/dev/null
  source /etc/os-release
  [[ "$ID" == ubuntu && ( "$VERSION_ID" == 24.04 || "$VERSION_ID" == 26.04 ) ]] || { printf 'Automatic dependency installation is limited to Ubuntu 24.04/26.04.\n' >&2; exit 1; }
  sudo apt-get update
  sudo apt-get install -y build-essential pkg-config curl ca-certificates xvfb x11vnc xauth x11-utils
fi
command -v cc >/dev/null || { printf 'A C compiler is required. See docs/install.md or use --install-dependencies on supported Ubuntu.\n' >&2; exit 1; }
# shellcheck source=ops/distribution/build-tools.sh
source "$script_dir/../ops/distribution/build-tools.sh"
install_build_tools "$PWD"
if [[ "$tools_only" == 1 ]]; then printf 'For this shell: export PATH=%q:"$PATH"\n' "$JAILGUN_BUILD_TOOLS_PATH"; exit 0; fi
output="target/distribution/bootstrap-$(date -u +%Y%m%dT%H%M%SZ)-$$"
arguments=(--out "$output")
if [[ "$allow_dirty" == 1 ]]; then arguments+=(--allow-dirty); fi
node scripts/build-bundle.mjs "${arguments[@]}"
archives=("$output"/jailgun-*.tar.gz)
[[ ${#archives[@]} == 1 && -f "${archives[0]}" ]]
bash scripts/install.sh --archive "${archives[0]}" --checksums "$output/SHA256SUMS" --prefix "$prefix"

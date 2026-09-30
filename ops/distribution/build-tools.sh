#!/usr/bin/env bash
# Source this file and call install_build_tools with the checkout root.
set -euo pipefail

distribution_sha256() {
  local result
  if command -v sha256sum >/dev/null; then result="$(sha256sum -- "$1")";
  else result="$(shasum -a 256 -- "$1")"; fi
  printf '%s\n' "${result%% *}"
}
distribution_download() {
  local url="$1" path="$2" expected="$3"
  if [[ ! -f "$path" ]]; then
    curl --fail --location --proto '=https' --proto-redir '=https' --tlsv1.2 --retry 2 --max-time 300 --output "$path.partial" "$url"
    [[ "$(distribution_sha256 "$path.partial")" == "$expected" ]] || { printf 'Download checksum mismatch: %s\n' "$path" >&2; return 1; }
    mv "$path.partial" "$path"
  fi
  [[ "$(distribution_sha256 "$path")" == "$expected" ]] || { printf 'Cached download checksum mismatch: %s\n' "$path" >&2; return 1; }
}
install_build_tools() {
  local root="$1" program platform syft_platform tool_root node_version node_name node_sha node_root syft_version syft_name syft_sha auditable
  for program in cargo rustc git curl tar awk; do
    command -v "$program" >/dev/null || { printf 'Missing build prerequisite: %s. See docs/install.md.\n' "$program" >&2; return 1; }
  done
  command -v sha256sum >/dev/null || command -v shasum >/dev/null || { printf 'Install sha256sum or shasum.\n' >&2; return 1; }
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) platform=linux-x64; syft_platform=linux_amd64 ;;
    Darwin-arm64) platform=darwin-arm64; syft_platform=darwin_arm64 ;;
    Darwin-x86_64) platform=darwin-x64; syft_platform=darwin_amd64 ;;
    *) printf 'No verified build tools for this platform.\n' >&2; return 1 ;;
  esac
  tool_root="$root/target/build-tools"
  mkdir -p "$tool_root/bin" "$tool_root/downloads"
  node_version="$(cat "$root/.node-version")"
  node_name="node-v$node_version-$platform.tar.gz"
  node_sha="$(awk -v name="$node_name" '$2 == name { print $1 }' "$root/ops/distribution/node-checksums.txt")"
  [[ "$node_sha" =~ ^[a-f0-9]{64}$ ]]
  distribution_download "https://nodejs.org/dist/v$node_version/$node_name" "$tool_root/downloads/$node_name" "$node_sha"
  node_root="$tool_root/node-v$node_version-$platform"
  if [[ ! -d "$node_root" ]]; then
    local stage
    stage="$(mktemp -d "$tool_root/node-stage.XXXXXXXX")"
    tar -xzf "$tool_root/downloads/$node_name" --strip-components=1 -C "$stage"
    [[ "$("$stage/bin/node" --version)" == "v$node_version" ]]
    mv "$stage" "$node_root"
  fi
  [[ "$("$node_root/bin/node" --version)" == "v$node_version" ]]
  JAILGUN_BUILD_TOOLS_PATH="$node_root/bin:$tool_root/bin"
  export PATH="$JAILGUN_BUILD_TOOLS_PATH:$PATH"
  syft_version="$(node -p 'JSON.parse(require("node:fs").readFileSync(process.argv[1])).syft' "$root/ops/distribution/tools.json")"
  syft_name="syft_${syft_version}_${syft_platform}.tar.gz"
  syft_sha="$(awk -v name="$syft_name" '$2 == name { print $1 }' "$root/ops/distribution/tool-checksums.txt")"
  [[ "$syft_sha" =~ ^[a-f0-9]{64}$ ]]
  distribution_download "https://github.com/anchore/syft/releases/download/v$syft_version/$syft_name" "$tool_root/downloads/$syft_name" "$syft_sha"
  tar -xzf "$tool_root/downloads/$syft_name" -C "$tool_root/bin" syft
  auditable="$(node -p 'JSON.parse(require("node:fs").readFileSync(process.argv[1])).cargo_auditable' "$root/ops/distribution/tools.json")"
  export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
  export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-line-tables-only}"
  cargo install --locked --version "$auditable" --root "$tool_root" --target-dir "$root/target" cargo-auditable
  printf 'Pinned build tools are available in %s\n' "$tool_root" >&2
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  distribution_script_dir="$(cd "$(dirname "$0")" && pwd)"
  install_build_tools "$(cd "$distribution_script_dir/../.." && pwd)"
  printf 'For this shell: export PATH=%q:"$PATH"\n' "$JAILGUN_BUILD_TOOLS_PATH"
fi

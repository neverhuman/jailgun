#!/usr/bin/env bash
set -euo pipefail
umask 077

die() { printf 'jailgun install: %s\n' "$*" >&2; exit 1; }
usage() {
  printf '%s\n' 'Usage: bash install.sh --version VERSION [--prefix DIRECTORY]' \
    '       bash install.sh --archive FILE --checksums FILE [--prefix DIRECTORY]' \
    'Downloads only from neverhuman/jailgun. Installs into ~/.local by default.' \
    'Runtime profiles, credentials and results are preserved. No privileged dependency installation.'
}
prefix="${HOME:?HOME is required}/.local"
version=""; archive=""; checksums=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix|--version|--archive|--checksums)
      [[ $# -ge 2 && -n "$2" && "$2" != --* ]] || die "$1 requires a value"
      case "$1" in
        --prefix) prefix="$2" ;;
        --version) version="$2" ;;
        --archive) archive="$2" ;;
        --checksums) checksums="$2" ;;
      esac
      shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) usage >&2; die "unknown option: $1" ;;
  esac
done
[[ -z "$version" || "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.-]+)?$ ]] || die 'invalid release version'
if [[ -n "$archive" || -n "$checksums" ]]; then
  [[ -n "$archive" && -n "$checksums" && -z "$version" ]] || die 'use --archive with --checksums, or --version'
else
  [[ -n "$version" ]] || die 'select an explicit --version'
fi
for program in tar awk sort uniq mktemp; do command -v "$program" >/dev/null || die "missing prerequisite: $program"; done
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) platform=linux-x64 ;;
  Darwin-arm64) platform=darwin-arm64 ;;
  Darwin-x86_64) platform=darwin-x64 ;;
  *) die 'supported bundles: Linux x86-64 and macOS ARM64/Intel' ;;
esac
cache="${XDG_CACHE_HOME:-$HOME/.cache}/jailgun/install"
mkdir -p "$cache"
stage="$(mktemp -d "$cache/stage.XXXXXXXX")"
# Keep the verified download and diagnostics until explicitly removed by the operator.
printf 'Installer staging directory: %s\n' "$stage" >&2
if [[ -n "$version" ]]; then
  command -v curl >/dev/null || die 'missing prerequisite: curl'
  archive="$stage/jailgun-$version-$platform.tar.gz"
  checksums="$stage/SHA256SUMS"
  base="https://github.com/neverhuman/jailgun/releases/download/v$version"
  curl --fail --location --proto '=https' --proto-redir '=https' --tlsv1.2 --retry 2 --max-time 180 --output "$checksums" "$base/SHA256SUMS"
  curl --fail --location --proto '=https' --proto-redir '=https' --tlsv1.2 --retry 2 --max-time 600 --output "$archive" "$base/$(basename "$archive")"
fi
[[ -f "$archive" && -f "$checksums" ]] || die 'archive or checksum file is missing'
name="$(basename "$archive")"
[[ "$name" =~ ^jailgun-[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.-]+)?-(linux-x64|darwin-arm64|darwin-x64)\.tar\.gz$ ]] || die 'unexpected archive name'
[[ "$name" == *-"$platform".tar.gz ]] || die 'archive belongs to another platform'
expected="$(awk -v name="$name" '$2 == name { print $1; count++ } END { if (count != 1) exit 1 }' "$checksums")" || die 'expected exactly one matching checksum'
[[ "$expected" =~ ^[a-f0-9]{64}$ ]] || die 'invalid SHA-256 entry'
if command -v sha256sum >/dev/null; then
  actual="$(sha256sum -- "$archive")"
elif command -v shasum >/dev/null; then
  actual="$(shasum -a 256 -- "$archive")"
else die 'install sha256sum or shasum before installing Jailgun'; fi
[[ "${actual%% *}" == "$expected" ]] || die 'archive checksum mismatch; nothing installed'
tar -tzf "$archive" > "$stage/members.txt"
tar -tvzf "$archive" > "$stage/types.txt"
awk 'BEGIN { valid=1 } !/^\.\/[A-Za-z0-9_@.+\/-]+$/ { valid=0 } { n=split(substr($0,3),parts,"/"); for(i=1;i<=n;i++) if(parts[i]=="" || parts[i]=="." || parts[i]=="..") valid=0 } END { exit !(valid && NR > 0 && NR < 100000) }' "$stage/members.txt" || die 'unsafe archive paths'
[[ -z "$(sort "$stage/members.txt" | uniq -d)" ]] || die 'duplicate archive paths'
awk 'substr($0,1,1) != "-" { exit 1 }' "$stage/types.txt" || die 'links and special archive files are forbidden'
mkdir "$stage/bundle"
tar -xzf "$archive" -C "$stage/bundle"
node="$stage/bundle/lib/jailgun/node/bin/node"
[[ -x "$node" && -f "$stage/bundle/lib/jailgun/install-bundle.mjs" ]] || die 'incomplete installation bundle'
"$node" "$stage/bundle/lib/jailgun/install-bundle.mjs" "$stage/bundle" "$prefix" "$expected"
prefix="$(cd "$prefix" && pwd -P)"
printf 'Add the installed commands to this shell:\n  export PATH=%q:"$PATH"\nThen run:\n  jailgun doctor\n  jailgun setup\n' "$prefix/bin"
printf '%s\n' 'Existing daemons retain their version until restarted; application data has not moved.'

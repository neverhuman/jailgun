#!/usr/bin/env bash
# Explicit CI dependency installation. No privileged package-manager calls.
set -euo pipefail
source "$(dirname "$0")/lib.sh"
ci_enter_repo_root "$(dirname "$0")"
ci_require_cmd cargo
ci_require_cmd python3
ci_require_cmd curl
ci_require_cmd sha256sum
ci_require_cmd tar

export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-line-tables-only}"
tool_root="$PWD/target/security-tools"
mkdir -p "$tool_root/bin" "$tool_root/downloads"
export PATH="$tool_root/bin:$tool_root/python/bin:$PATH"

if ! cargo-audit --version 2>/dev/null | grep -qx 'cargo-audit 0.22.1'; then
  cargo install --locked --version 0.22.1 --root "$tool_root" --target-dir "$PWD/target" cargo-audit
fi
if ! cargo-deny --version 2>/dev/null | grep -qx 'cargo-deny 0.19.8'; then
  cargo install --locked --version 0.19.8 --root "$tool_root" --target-dir "$PWD/target" cargo-deny
fi
if ! zizmor --version 2>/dev/null | grep -qx 'zizmor 1.25.2'; then
  python3 -m venv "$tool_root/python"
  "$tool_root/python/bin/pip" install 'zizmor==1.25.2'
fi

install_archive() {
  local repository="$1" version="$2" archive="$3" checksums="$4" binary="$5"
  local url="https://github.com/$repository/releases/download/v$version"
  local download_dir="$tool_root/downloads/$binary-$version"
  mkdir -p "$download_dir"
  curl --fail --location --retry 3 "$url/$archive" --output "$download_dir/$archive"
  curl --fail --location --retry 3 "$url/$checksums" --output "$download_dir/checksums.txt"
  (
    cd "$download_dir"
    awk -v name="$archive" '$2 == name || $2 == "*" name {print}' checksums.txt > selected.sha256
    test -s selected.sha256
    sha256sum --check selected.sha256
    tar -xzf "$archive" -C "$tool_root/bin" "$binary"
  )
}

case "$(uname -s)/$(uname -m)" in
  Linux/x86_64) ;;
  *) printf 'Binary scanner installation currently requires Linux x86-64. Install the pinned tools manually on this platform.\n' >&2; exit 2 ;;
esac
if ! gitleaks version 2>/dev/null | grep -qx '8.21.2'; then
  install_archive gitleaks/gitleaks 8.21.2 gitleaks_8.21.2_linux_x64.tar.gz gitleaks_8.21.2_checksums.txt gitleaks
fi
if ! syft version 2>/dev/null | grep -q '^Version: *1.40.0$'; then
  install_archive anchore/syft 1.40.0 syft_1.40.0_linux_amd64.tar.gz syft_1.40.0_checksums.txt syft
fi
if ! actionlint -version 2>/dev/null | head -n 1 | grep -qx '1.7.12'; then
  install_archive rhysd/actionlint 1.7.12 actionlint_1.7.12_linux_amd64.tar.gz actionlint_1.7.12_checksums.txt actionlint
fi

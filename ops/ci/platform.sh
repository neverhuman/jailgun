#!/usr/bin/env bash
# Full synthetic compatibility check on each intended distribution host.
set -euo pipefail
source "$(dirname "$0")/lib.sh"
ci_enter_repo_root "$(dirname "$0")"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-line-tables-only}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export JAILGUN_WORKERS="${JAILGUN_WORKERS:-2}"
if [[ -z "${JAILGUN_TEST_CHROME:-}" ]]; then
  case "$(uname -s)" in
    Linux) JAILGUN_TEST_CHROME="$(command -v google-chrome || command -v chromium || true)" ;;
    Darwin) JAILGUN_TEST_CHROME='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' ;;
  esac
fi
[[ -n "${JAILGUN_TEST_CHROME:-}" && -x "$JAILGUN_TEST_CHROME" ]] || { ci_warn 'Install Chrome or set JAILGUN_TEST_CHROME; browser sandboxing is required.'; exit 1; }
export JAILGUN_TEST_CHROME
bash ops/ci/rust.sh
bash ops/ci/node.sh
bash ops/ci/db.sh
bash ops/ci/e2e.sh
bash ops/ci/ux-qa.sh
bash ops/ci/docs.sh
source ops/distribution/build-tools.sh
install_build_tools "$PWD"
bash ops/ci/package.sh

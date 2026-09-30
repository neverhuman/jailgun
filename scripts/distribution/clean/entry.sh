#!/usr/bin/env bash
set -euo pipefail
umask 077
export HOME=/home/test
export PATH=/usr/bin:/bin
cd /download
sha256sum --check --ignore-missing SHA256SUMS
cd "$HOME"
bash /download/install.sh --archive "/download/${1:?archive filename required}" --checksums /download/SHA256SUMS --prefix "$HOME/installed application"
"$HOME/installed application/lib/jailgun/current/lib/jailgun/node/bin/node" /proof/smoke.mjs

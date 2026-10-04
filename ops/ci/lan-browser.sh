#!/usr/bin/env bash
# Prepare the locked browser in a disposable LAN guest, including its real sandbox.
set -euo pipefail
case "$(systemd-detect-virt)" in
  kvm|qemu) ;;
  *) echo 'LAN browser preparation requires a disposable VM' >&2; exit 2 ;;
esac
[[ $(node -p 'require("playwright-core/package.json").version') == 1.60.0 ]] || {
  echo 'Update the pinned browser artifact when the Playwright lock changes' >&2; exit 2;
}
image=mcr.microsoft.com/playwright/python@sha256:abf13b369f8829eb45e29df38d6c5221f7e7521649cb5d2de7989c82bdb574ad
docker pull --platform linux/amd64 "$image"
container=$(docker create "$image")
trap 'docker rm -f "$container" >/dev/null 2>&1 || true' EXIT
browser_cache=${PLAYWRIGHT_BROWSERS_PATH:-$HOME/.cache/ms-playwright}
mkdir -p "$browser_cache"
docker cp "$container:/ms-playwright/." "$browser_cache/"
sudo chown -R "$(id -u):$(id -g)" "$browser_cache"
docker rm "$container"
trap - EXIT
docker image rm "$image"
node node_modules/playwright-core/cli.js install --with-deps chromium
# Ubuntu's generic AppArmor user-namespace restriction otherwise prevents the
# Chromium sandbox for this unpacked binary. This changes only the isolated VM.
sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0
browser=$(node -p 'require("playwright-core").chromium.executablePath()')
[[ -x $browser ]] || exit 1
export JAILGUN_TEST_CHROME="$browser"
node <<'NODE'
const { chromium } = require('playwright-core');
(async () => {
  const browser = await chromium.launch({
    executablePath: process.env.JAILGUN_TEST_CHROME,
    headless: true,
    chromiumSandbox: true,
  });
  const page = await browser.newPage();
  await page.goto('data:text/html,<title>LAN browser sandbox</title>');
  if (await page.title() !== 'LAN browser sandbox') throw new Error('Browser launch failed');
  console.log(`Sandboxed Chromium ${browser.version()} launched successfully`);
  await browser.close();
})().catch(error => { console.error(error); process.exitCode = 1; });
NODE
if [[ -n ${GITHUB_ENV:-} ]]; then
  printf 'JAILGUN_TEST_CHROME=%s\n' "$browser" >> "$GITHUB_ENV"
fi

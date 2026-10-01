# Testing and Proof Lanes

Use `agent/test-map.json` for the narrow proof route for a changed path. The
local parity entry point is:

```bash
bash scripts/ci-local.sh
```

## Lanes

- Rust: `bash ops/ci/rust.sh`
- Node: `bash ops/ci/node.sh`
- Security: `bash ops/ci/security.sh`
- Contracts: `bash ops/ci/contracts.sh`
- Rendered UX: `bash ops/ci/ux-qa.sh`
- Copy-code: `bash ops/ci/copy-code.sh`
- Release readiness: `bash ops/ci/release.sh`
- Audit: `bash ops/ci/jankurai.sh`
- Doctor: `bash scripts/ci-doctor.sh`
- Database: `bash ops/ci/db.sh`
- Browser workflows: `bash ops/ci/e2e.sh`
- Documentation: `bash ops/ci/docs.sh`
- Native distribution: `bash ops/ci/package.sh`
- Worker MCP mock API: `bash ops/ci/worker.sh`

The worker lane uses an injected fake executor and covers official-SDK MCP
negotiation, operator-only tool discovery, multi-account readiness and routing,
private metadata permissions, tabs, model/effort controls, idempotency,
cancellation, timeout behavior and verified tar object round trips. It never
starts Codex, Chrome, external setup, or a live model.

The E2E lane runs the production Rust scheduler and account bridge with real
sandboxed Chrome against an interactive synthetic provider: five/ten candidates,
all reduction stages through HTTP, CLI, MCP and the built dashboard, accepted-turn
restart recovery, shared-account cancellation, run/response deadline stops and
provider-timestamp cooldowns, including uncertain acceptance without resubmission.
Only the fixture clock is accelerated; submission spacing and
budgets are still persisted and checked. This is not live-provider acceptance.

The Rust lane executes generated deployment shell scripts against disposable
Git repositories beneath `target/`, including preservation failures and checkout
mutation ownership. See [advanced deployment safety](advanced-deploy-safety.md).

## Repair Errors

Agent-readable errors include `purpose`, `reason`, common fixes, `docs_url`,
and `repair_hint`. The purpose names the boundary that failed, the reason gives
the stable machine-readable cause, common fixes list the smallest likely local
repairs, `docs_url` points to this file or a more specific owner document, and
`repair_hint` names the next rerun command.

## Contracts

Contract artifacts are governed outputs. `bash ops/ci/contracts.sh` checks
schema and fixture drift. `bash ops/ci/contracts.sh --write` refreshes the
schema and fixtures from `scripts/generate-contracts.mjs`. Rust-derived
standalone validators check browser API boundaries. The workflow fixture example
executes SQLite account and run transitions, then normalizes generated identities
for deterministic shared demo and recovery-state tests.
Every canonical archive event kind also has a generated JSON fixture. The
generated fixture index imports those files into the dashboard tests, which
send each fixture through the production WebSocket decoder and check the
delivered payload. Adding an event kind without a fixture fails this coverage.

## Rendered UX

`bash ops/ci/ux-qa.sh` builds the dashboard and executes Chromium through
Playwright with Chrome's sandbox enabled. Install the matching test browser with
`node node_modules/playwright-core/cli.js install chromium`, or select an installed
sandbox-capable Chrome with `JAILGUN_TEST_CHROME=/path/to/chrome`.

The lane checks loading, empty, API-error, success, pairing, and explicit synthetic demo
states at desktop and mobile sizes, plus concept onboarding, model confirmation,
form input, complete/partial progress, final output and comparison tables. It records real PNG screenshots, ARIA
snapshots, axe accessibility results, keyboard navigation, horizontal overflow,
and console/network errors under `artifacts/ux-qa/`. The report at
`target/jankurai/ux-qa.json` includes source identity, dirty status, current times,
exit status, browser/Node versions, and hashes of the source, built assets and
artifact contents. Changing an input during the checks fails the lane. A missing
browser or failed check fails the lane. Human visual review is separate; the
report does not claim a baseline match or measurements it did not execute.

The browser measures cumulative layout shift (CLS) while synthetic API responses
arrive after an 800 ms delay. It records actual `layout-shift` entries and requires
the largest session window to stay at or below 0.1 in each tested state. A separate
browser control deliberately displaces visible content and must exceed the budget,
so unsupported or broken instrumentation cannot report a pass. The calculation
excludes recent input and uses the five-second window/one-second gap definition
described in [the CLS reference](https://web.dev/articles/cls). This is a synthetic
regression budget, not a claim about field performance or live accounts.

See [dashboard design and visual review](dashboard-design.md) for the shared
tokens, interaction requirements and review procedure.

## Budgets and Stop Conditions

CI lanes must not require real Google, GitHub write, SSH, Telegram, or browser
profile credentials. Paid or unbounded remote work stops when credentials are
missing, when a lane needs private runtime state, when a generated artifact
would require a hand edit, or when a command exceeds its GitHub job timeout.
Local agents should report the failing lane, the last artifact path, and the
next rerun command instead of broadening scope.

Cost-bearing work has a zero-default budget in CI: paid API calls are disabled
unless a reviewer records a budget, quota, spend cap, and kill switch for the
run. The stop condition is any missing quota evidence, any exhausted budget,
any missing kill-switch owner, or any lane that would need private runtime
credentials.

## Launch Gates

Release evidence must cover security scans, backup or preservation behavior,
monitoring and audit artifacts, rollback instructions, and abuse controls for
prompt/tool agency. The documentation control and advisory audit lanes are
preliminary checks, not release acceptance. `release.sh` explicitly reports
`release_verified: false`. Final-source integrated CI, verified packages, clean
installation and live provider checks remain required; see
[release procedure](release.md).

## Managed Linux login viewer

The Linux E2E lane requires Xvfb, x11vnc and xdpyinfo (`x11-utils` on Ubuntu).
It executes `private_display_and_rfb_isolation`, including wrong-account
X authority rejection, separate display allocation, exclusive viewers, private
RFB initialization, no X TCP listener and parent-death cleanup.
`node scripts/check-concept-dashboard.mjs viewer` uses real noVNC keyboard
input to log into the synthetic provider. It checks viewer cancellation, expiry,
reconnect, identity/model confirmation and a full five-candidate concept run.
The desktop/mobile login view gets actual screenshots, axe checks and keyboard
focus assertions. This lane does not establish live ChatGPT compatibility.

## Clean Linux installation

On Linux, the package lane requires Docker access from a non-root user. It runs
the verified archive in the Ubuntu image pinned by digest in
`ops/distribution/clean-linux.json`. Only the selected archive, installer,
checksums and proof scripts are mounted; the checkout and host runtime are
absent. The container has no network, no added capabilities, a read-only base
filesystem and a private executable home directory.

The check verifies the absence of Rust, Cargo, npm, system Node and Git, then
exercises setup, pairing, private APIs, dashboard assets, credential/daemon
reuse, authenticated shutdown and offline backup/restore. With Chrome absent,
`doctor` must identify that prerequisite while verifying the bundled assets.
The real browser and native service-manager checks run in separate lanes.
This container check does not establish first login, a full concept workflow,
macOS installation or live-provider compatibility.

To repeat it on an existing Linux candidate:

```bash
node scripts/distribution/clean/host.mjs target/distribution/CANDIDATE/jailgun-0.2.0-linux-x64.tar.gz
```

The private `target/clean-linux-proofs/` report records the archive hash, actual
image identity, proof-tool hashes, candidate source and executed checks.
CI uploads only the sanitized `evidence.json`; container output, runtime state
and credentials are not uploaded. Each uniquely labelled container is removed
after the check, including failed runs. No other containers are changed.

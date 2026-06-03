# Dual-Browser Runbook: Two Google Profiles, One Run

This runbook walks through driving jailgun across **two real Google
profiles concurrently**, so a `--tabs N` run fans out across both
ChatGPT sessions in parallel. Everything jailgun needs is implemented;
the only manual step is logging each profile into ChatGPT once.

## Why two profiles

- Each Google account gets its own ChatGPT rate budget. Two profiles
  ≈ double the throughput on long batches.
- Jailgun rotates tabs round-robin across the pool, so tab 1, 3, 5…
  use profile A and tab 2, 4, 6… use profile B.
- Every event jailgun emits carries `browser_profile`,
  `browser_profile_dir`, `browser_slot`, and `cdp_url` so the dashboard
  and any post-hoc analysis can attribute work back to the right
  identity.

## Prerequisites

- Chrome installed (managed at `/Applications/Google Chrome.app/...`
  on macOS, or first found in `PATH` on Linux).
- jailgun built (`cargo build -p jailgun-cli --bin jailgun`).
- JMCP bridge running for production wrapper notifications (`node
  ~/code/jmcp/bin/jmcp-bridge.mjs`).

## Production setup

### 1. Create the profile directories

```bash
mkdir -p ~/.jailgun/profiles/prod-google-a
mkdir -p ~/.jailgun/profiles/prod-google-b
```

These are dedicated Chrome `--user-data-dir` roots for this production
proof. Do not use a normal daily Chrome profile.

### 2. Confirm CDP ports are free

The production wrapper owns managed Chrome on `127.0.0.1:9224` and
`127.0.0.1:9225` by default.

```bash
lsof -nP -iTCP:9224 -sTCP:LISTEN
lsof -nP -iTCP:9225 -sTCP:LISTEN
```

Stop immediately if either command shows a listener.

### 3. Log each profile into ChatGPT once

Open one headed Chrome window at a time with the login helper. Do not
use the chrome-bridge for manual login.

```bash
bash scripts/login-profile-interactive.sh ~/.jailgun/profiles/prod-google-a
```

For profile A, sign in with the first Google/OpenAI account, confirm
the ChatGPT composer is visible, and close Chrome completely. Then
repeat with profile B and a different Google/OpenAI account:

```bash
bash scripts/login-profile-interactive.sh ~/.jailgun/profiles/prod-google-b
```

If a profile is not fully logged in, jailgun will hit the
missing-composer error on every tab that lands on that slot (visible in
events.ndjson as `Error · missing chat control: ...`).

### 4. Write a local login receipt

Record the manual login result without emails, account names, cookies,
screenshots with account identifiers, or secrets.

```bash
run_id="prod-two-profile-$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "artifacts/live-runs/$run_id"
cat > "artifacts/live-runs/$run_id/manual-login-receipt.txt" <<EOF
manual login receipt
created_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
profile_a=$HOME/.jailgun/profiles/prod-google-a
profile_a_composer_visible=yes
profile_b=$HOME/.jailgun/profiles/prod-google-b
profile_b_composer_visible=yes
profiles_use_distinct_google_openai_users=yes
account_identifiers_recorded=no
EOF
```

## Staged production proof

Use `scripts/run-live-2profile.sh`. It sources `.env.live-7tab`, passes
both profile dirs via repeated `--profile-pool`, launches headed managed
Chrome from port `9224`, rejects fake-chatgpt/`127.0.0.1:8082`/`/new`
targets, and writes proof under `artifacts/live-runs/<run-id>/`.
When `JAILGUN_RUN_ID` is set, the wrapper appends the stage name
(`-canary`, `-full`, or `-deploy`) to create the actual run id.

Set or verify these local env values in `.env.live-7tab`:

```bash
JAILGUN_PROFILE_A_DIR=$HOME/.jailgun/profiles/prod-google-a
JAILGUN_PROFILE_B_DIR=$HOME/.jailgun/profiles/prod-google-b
JAILGUN_CDP_PORT_START=9224
# Optional. If unset, each stage writes state under its run artifact dir.
# JAILGUN_CHROME_STATE_DIR=$HOME/.jailgun/chrome-state/prod-two-profile
```

Before a real run, have the read-only reviewer check the wrapper
preflight output and record the review in `AGENT_CHAT.md`.

### Canary: 2 tabs, no deploy

```bash
JAILGUN_RUN_ID="$run_id" \
  scripts/run-live-2profile.sh --stage canary
```

Success requires one tab per profile, CDP URLs `http://127.0.0.1:9224`
and `http://127.0.0.1:9225`, two download receipts, two closed tabs,
zero error events, zero undismissed rate-limit events, and cleanup proof.

### Full browser proof: 30 planned tabs, no deploy

Run this only after the canary proof is green and reviewed:

```bash
JAILGUN_RUN_ID="$run_id" \
  scripts/run-live-2profile.sh --stage full
```

The default full stage is `--full-tabs 10 --full-loops 2`, for 30
planned tabs. Success requires a 15/15 split across the two profile dirs,
a 15/15 split across slots `1` and `2`, 30 download receipts, 30 closed
tabs, zero errors, and cleanup proof.

### Deploy proof

Run deploy only after the no-deploy browser proof is green and reviewed:

```bash
JAILGUN_RUN_ID="$run_id" \
  scripts/run-live-2profile.sh --stage deploy --enable-deploy-stage
```

By default the deploy stage uses `JAILGUN_DEPLOY_TABS` and
`JAILGUN_DEPLOY_LOOPS`, falling back to `JAILGUN_TABS` and
`JAILGUN_LOOPS` from `.env.live-7tab`. Override explicitly when budget
requires it:

```bash
scripts/run-live-2profile.sh \
  --stage deploy \
  --enable-deploy-stage \
  --deploy-tabs 10 \
  --deploy-loops 2
```

Deploy success requires the browser invariants above plus deploy receipts,
expected remote host and command matches, remote local CI success as
configured, and cleanup proof.

### All browser stages, then optional deploy

```bash
scripts/run-live-2profile.sh --stage all
```

This runs canary first and then full browser proof. It skips deploy unless
you pass `--enable-deploy-stage`; if canary or full fails, later stages do
not run.

## Confirming both browsers are running

While the run is in flight:

```bash
# 1. Two Chrome processes with the right --user-data-dir values
pgrep -af 'user-data-dir=.*profiles/prod-google-[ab]'

# 2. managed-browsers.json shows two slots on sequential ports
jq '.profiles[] | {slot, cdp_url, profile_name}' \
  artifacts/live-runs/<run-id>/chrome-state/managed-browsers.json

# 3. Live events.ndjson shows tab-opened per slot
grep '"kind":"tab-opened"' artifacts/live-runs/<run-id>/events.ndjson \
  | jq '{tab_id, browser_slot: .fields.browser_slot, browser_profile: .fields.browser_profile}'

# 4. Dashboard at http://127.0.0.1:8787 shows one row per tab.
```

## Scaling beyond two

The same flag pattern fans out to any pool size:

```bash
--profile-pool ~/.jailgun/profiles/prod-google-a \
--profile-pool ~/.jailgun/profiles/prod-google-b \
--profile-pool ~/.jailgun/profiles/prod-google-c
```

CDP ports allocate sequentially up to `9234`
(`apps/chrome-bridge/bin/chrome-bridge.mjs` `MANAGED_CDP_MAX_PORT`), so
up to 11 simultaneous managed browsers per box without conflicting
with anything else. Beyond that you'd need to bump the cap and confirm
no other process owns those ports.

## Hermetic CI proof

The `ops/ci/e2e-fake-chatgpt.sh` lane already exercises the dual-profile
path against the in-repo fake-chatgpt simulator and asserts:

- 10 `tab-opened` events
- 30 planned `tab-opened`, `archive-uploaded`, and `prompt-submitted`
  events by default (`10` tabs with `loops=2`)
- 15 per `browser_profile_dir`
- exactly 2 `cdp_url` values (`9224` + `9225`)
- per-slot time windows overlap (proving the two browsers are running
  in parallel, not serialized)
- bundled prompt injection during source upload for every planned tab
- no leftover fake-chatgpt, CDP, or artifact-open-file processes after
  cleanup

The lane writes the proof to
`target/e2e-fake-chatgpt/profile-proof.json`. Re-run with:

```bash
bash ops/ci/e2e-fake-chatgpt.sh
```

For real-life runs you do NOT need this lane — it just guards against
regressions in the routing layer.

## Cleanup

`scripts/run-live-2profile.sh` treats cleanup as part of the proof. After
the runner exits, it writes `cleanup-proof.json` and fails unless:

- no managed Chrome process remains for either profile dir
- CDP ports `9224` and `9225` are no longer listening
- `managed-browsers.json` records both profile slots as stopped
- each profile-scoped `managed-browser.json` records `status: stopped`

The final receipt points at both `monitor-proof.json` and
`cleanup-proof.json`.

## Troubleshooting

- **`missing chat control: #prompt-textarea`** — the slot's profile
  isn't logged into ChatGPT. Re-do the headed step for that profile.
- **`address already in use 127.0.0.1:9224`** — another process owns
  the default CDP port. Kill it, or set `JAILGUN_CDP_PORT_START` to a
  free two-port range and verify the monitor expects the same URLs.
- **Only one Chromium spawns** — verify the run command actually
  contains `--profile-pool` twice (or that
  `JAILGUN_CHROME_PROFILE_POOL` has both paths separated by `:` on
  Unix / `;` on Windows). A single `--profile-pool` is single-browser.
- **One slot does all the work** — round-robin needs `--tabs >= pool
  size`. With `--tabs 1` only slot 1 gets a tab.

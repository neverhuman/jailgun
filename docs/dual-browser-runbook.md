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
- JMCP bridge running for notifications (optional — `node
  ~/code/jmcp/bin/jmcp-bridge.mjs`).

## One-time setup

### 1. Create the profile directories

```bash
mkdir -p ~/.jailgun/profiles/google-a
mkdir -p ~/.jailgun/profiles/google-b
```

The bridge will use these as `--user-data-dir` for the managed Chrome
processes. They must exist before the run; jailgun does not create
them.

### 2. Log each profile into ChatGPT once (HEADED)

For each profile, open a one-shot headed Chrome that uses ONLY that
profile dir, sign in, and close. The bridge keeps the login cookies in
the profile dir.

```bash
# Profile A
node apps/chrome-bridge/bin/chrome-bridge.mjs \
  --headed \
  --profile-pool ~/.jailgun/profiles/google-a
```

When Chrome opens:

1. Navigate to https://chatgpt.com.
2. Sign in with the first Google account.
3. Make sure ChatGPT loads to the composer (not stuck on a login
   wall).
4. Close Chrome. The bridge process can be killed with Ctrl-C.

Repeat for profile B:

```bash
node apps/chrome-bridge/bin/chrome-bridge.mjs \
  --headed \
  --profile-pool ~/.jailgun/profiles/google-b
```

If you skip a profile or one isn't fully logged in, jailgun will hit
the missing-composer error on every tab that lands on that slot
(visible in events.ndjson as `Error · missing chat control: ...`).

### 3. (Optional) Confirm `managed-browsers.json`

After step 2, you should see one entry per profile inspected so far:

```bash
cat ~/.google-profile-automation-state/managed-browsers.json
```

Each entry records the CDP URL the bridge will use next time
(`http://127.0.0.1:9224`, `9225`, …).

## Dual-profile run

```bash
cargo run -p jailgun-cli --bin jailgun -- run \
  --prompt-file prompts/templates/harden-repo.txt \
  --tabs 2 \
  --profile-pool ~/.jailgun/profiles/google-a \
  --profile-pool ~/.jailgun/profiles/google-b \
  --notify-jmcp \
  --jmcp-inbox-dir ~/code/jmcp/inbox \
  --serve
```

What happens:

1. Jailgun launches the chrome-bridge with both profile dirs.
2. The bridge spawns **two managed Chromiums**, one per profile, on
   sequential CDP ports starting at `9224`.
3. The orchestrator round-robins tabs: tab 1 goes to profile A
   (CDP `9224`), tab 2 goes to profile B (CDP `9225`).
4. Every `tab-opened` event carries the slot's identity:
   `browser_profile`, `browser_profile_dir`, `browser_slot`, `cdp_url`.
5. The dashboard (served on `http://127.0.0.1:8787` because of
   `--serve`) shows one row per tab with the profile label next to
   the tab id.
6. If `--notify-jmcp` is set, the JMCP bridge ships a Telegram message
   for each milestone (job started, tar acquired, deploy succeeded,
   ...) with the profile metadata in the structured payload.

## Confirming both browsers are running

While the run is in flight:

```bash
# 1. Two Chrome processes with the right --user-data-dir values
pgrep -af 'user-data-dir=.*profiles/google-[ab]'

# 2. managed-browsers.json shows two slots on sequential ports
jq '.profiles[] | {slot, cdp_url, profile_name}' \
  ~/.google-profile-automation-state/managed-browsers.json

# 3. Live events.ndjson shows tab-opened per slot
grep '"kind":"tab-opened"' artifacts/live-runs/<run-id>/events.ndjson \
  | jq '{tab_id, browser_slot: .fields.browser_slot, browser_profile: .fields.browser_profile}'

# 4. Dashboard at http://127.0.0.1:8787 — TabRow shows
#    "google-a #1" / "google-b #2" pills next to each tab.
```

## Scaling beyond two

The same flag pattern fans out to any pool size:

```bash
--profile-pool ~/.jailgun/profiles/google-a \
--profile-pool ~/.jailgun/profiles/google-b \
--profile-pool ~/.jailgun/profiles/google-c
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
- 5 per `browser_profile_dir`
- exactly 2 `cdp_url` values (`9224` + `9225`)
- per-slot time windows overlap (proving the two browsers are running
  in parallel, not serialized)

The lane writes the proof to
`target/e2e-fake-chatgpt/profile-proof.json`. Re-run with:

```bash
bash ops/ci/e2e-fake-chatgpt.sh
```

For real-life runs you do NOT need this lane — it just guards against
regressions in the routing layer.

## Troubleshooting

- **`missing chat control: #prompt-textarea`** — the slot's profile
  isn't logged into ChatGPT. Re-do the headed step for that profile.
- **`address already in use 127.0.0.1:9224`** — another process owns
  the default CDP port. Kill it, or change
  `JAILGUN_MANAGED_CDP_PORT` to a free range.
- **Only one Chromium spawns** — verify the run command actually
  contains `--profile-pool` twice (or that
  `JAILGUN_CHROME_PROFILE_POOL` has both paths separated by `:` on
  Unix / `;` on Windows). A single `--profile-pool` is single-browser.
- **One slot does all the work** — round-robin needs `--tabs >= pool
  size`. With `--tabs 1` only slot 1 gets a tab.

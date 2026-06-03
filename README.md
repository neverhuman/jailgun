<img src="assets/jailgun.png" alt="Jailgun" width="100%">

<!-- jankurai-badge:start -->
[![Jankurai score: 70/100 advisory](agent/jankurai-badge.svg)](agent/repo-score.md)
<!-- jankurai-badge:end -->

# Jailgun

Jailgun runs authenticated ChatGPT tab batches, captures generated source
archives, validates receipts, and deploys through a Rust-owned safety layer.

The repository is intentionally example-first. Real credentials, browser
profiles, local paths, remotes, prompts, archives, logs, and receipts are local
runtime state and are ignored by Git.

## Layout

- `crates/jailgun-core` owns configuration, event models, tar validation,
  receipts, prompt policy, and repository string audits.
- `crates/jailgun-deploy` owns remote cleanup and deploy orchestration behind
  testable traits.
- `crates/jailgun-server` serves REST snapshots and WebSocket events.
- `crates/jailgun-cli` exposes config validation, tar validation, scanning, and
  dashboard serving.
- `apps/browser-adapter` contains DOM-only TypeScript helpers for the browser
  automation boundary.
- `apps/dashboard` is a Vite/React dashboard that works with fixture data.

## Local Validation

```bash
bash ops/ci/rust.sh
bash ops/ci/node.sh
bash ops/ci/security.sh
bash ops/ci/jankurai.sh
```

The Jankurai lane writes `agent/repo-score.{json,md}` plus
`agent/jankurai-badge.{svg,json}` and refreshes the badge block between the
README markers above.

## Configuration

Start from `config/jailgun.example.toml` and write local values to an ignored
`config/jailgun.local.toml` or environment variables. Use `.env.example` as the
environment reference.

### Dual Browser Profiles

For concurrent Google accounts, run separate managed Chrome user-data
directories instead of sharing one logged-in profile:

```bash
cargo run -p jailgun-cli -- run \
  --prompt-file prompts/templates/harden-repo.txt \
  --tabs 10 \
  --profile-pool ~/.jailgun/profiles/google-a \
  --profile-pool ~/.jailgun/profiles/google-b
```

The first launch opens one Chrome instance per profile on sequential local CDP
ports (`9224`, `9225`, ...). Log into the intended Google account in each
window once; future runs reuse the same ignored profile directories. Jailgun
writes tracking state to `~/.google-profile-automation-state/managed-browsers.json`
and per-profile receipts under `profiles/<name>/managed-browser.json`.

You can also set `JAILGUN_CHROME_PROFILE_POOL` or
`JAILGUN_CHROME_PROFILE_DIRS` to a path-list. Tab launches rotate through the
pool, and the dashboard shows the assigned browser profile, profile directory,
slot, and CDP URL for each tab.

The CI smoke lane mirrors that shape with 10 tabs per batch split 5/5 across
two artifact-scoped profile directories under `target/e2e-fake-chatgpt/`. It
checks three batches by default, two unique CDP URLs, overlapping slot windows,
source upload, bundled prompt injection, prompt submission, downloads, and
post-run cleanup.

Remote cleanup policy defaults to `preserve-reset`. Clean divergent remote
checkouts are preserved under a timestamped ref and receipt before reset.
Dirty checkouts, missing `origin/main`, failed ref creation, and failed receipt
writes stop the deploy.

## Notifications via JMCP (optional)

Jailgun does **not** talk to Telegram, Slack, or any user-facing channel
directly. Instead it writes JPCM/2.0 envelopes into a local outbox; a
separate JMCP bridge owns the bot token and ships the message. The
notifier is **optional** — jailgun runs end-to-end without it.

To enable it on your machine:

1. Make sure the JMCP repo is present at `~/code/jmcp/` (it ships the
   bridge stub at `~/code/jmcp/bin/jmcp-bridge.mjs`).
2. Copy `~/code/jmcp/.env.example` to `~/code/jmcp/.env` and fill in
   `TELEGRAM_BOT_TOKEN` and `TELEGRAM_CHAT_ID`. The file is gitignored
   and lives outside the jailgun tree.
3. Start the bridge once (or wire it into `launchd`):

   ```bash
   node ~/code/jmcp/bin/jmcp-bridge.mjs
   ```

4. From the jailgun repo, send a one-off envelope:

   ```bash
   cargo run -p jailgun-cli -- jmcp-send \
     --message "Jailgun online" \
     --title "Online" --summary-emoji "🛰"
   ```

   The CLI writes a `jpcm_*.json` envelope into
   `~/code/jmcp/inbox/`; the bridge picks it up within a few seconds,
   posts to Telegram, and moves the file to `~/code/jmcp/delivered/`.
   No bot credentials live in jailgun.

5. To request a batch of jailgun runs through JMCP, queue a structured
   batch request:

   ```bash
   cargo run -p jailgun-cli -- runs \
     --count 4 \
     --config config/jailgun.example.toml \
     --prompt-file prompts/templates/harden-repo.txt \
     --profile-pool ~/.jailgun/profiles/google-a \
     --profile-pool ~/.jailgun/profiles/google-b
   ```

   That writes a `jailgun.batch-request` envelope with
   `approval_required: true`, the child `jailgun run` command template,
   the config/prompt paths, and any profile-pool flags JMCP needs to
   create the approval gate and fan out the child work orders.

6. To ping on every successful local commit, install the post-commit
   hook:

   ```bash
   bash ops/ci/install-hooks.sh   # or copy ops/git-hooks/post-commit yourself
   ```

   The hook runs `jailgun notify-commit --jmcp-inbox-dir
   ~/code/jmcp/inbox`. If the JMCP repo isn't present, the hook exits
   quietly without failing the commit.

See `docs/jmcp-integration.md` for the full envelope contract.

Run history and deploy events also fan out over the dashboard's
WebSocket endpoint regardless of JMCP setup.

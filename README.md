<img src="assets/jailgun.png" alt="Jailgun" width="100%">

<!-- jankurai-badge:start -->
[![Jankurai score: 89/100 advisory](agent/jankurai-badge.svg)](agent/repo-score.md)
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

### Remote Chrome over SSH

Jailgun can attach to a Chrome instance that stays on another machine as long
as that machine keeps the Chrome DevTools Protocol bound to `127.0.0.1` and the
browser profile and state remain local there. The Mac only needs the forwarded
CDP endpoint, not the browser UI.

Use the helpers in `scripts/` to bring up a loopback-only SSH forward and keep
it alive with `launchd`:

- `scripts/chrome-cdp-tunnel.sh` opens `127.0.0.1:9224` on the Mac and forwards
  it to the remote machine's loopback CDP port.
- `scripts/chrome-cdp-launchd.sh` writes and loads a LaunchAgent so the tunnel
  restarts after login or disconnect.

When the tunnel is up, run `jailgun` or `chrome-bridge` on the Mac with
`JAILGUN_CDP_URL=http://127.0.0.1:9224`.

The tunnel helper uses SSH key authentication, `StrictHostKeyChecking=yes`, and
a pinned `known_hosts` file so the CDP endpoint is only exposed through the
trusted SSH connection.

Remote cleanup policy defaults to `preserve-reset`. Clean divergent remote
checkouts are preserved under a timestamped ref and receipt before reset.
Dirty checkouts, missing `origin/main`, failed ref creation, and failed receipt
writes stop the deploy.

## Telegram notifications (optional)

Jailgun can push a short message to a private Telegram chat each time a deploy
commit succeeds or fails. The notifier is **optional** — Jailgun runs end-to-end
without it.

To enable it on your machine:

1. Open Telegram and message `@BotFather`. Run `/newbot`, follow the prompts,
   and copy the bot token it gives you (a string of the form `1234567:ABC...`).
2. Put the token in a local file the repo will not commit:

   ```bash
   mkdir -p telegram
   printf '%s\n' '<paste-your-bot-token-here>' > telegram/token.env
   chmod 600 telegram/token.env
   ```

   The `telegram/` directory is listed in `.gitignore`, so the token, the
   discovered chat id cache, and any local notes you keep there stay out of
   Git.
3. Open your bot in Telegram (search for the bot name you chose in step 1) and
   send it `/start`. Bots cannot DM you until you DM them first.
4. Send the first test message from the repo root:

   ```bash
   cargo run -p jailgun-cli -- telegram-send \
     --token-file telegram/token.env \
     --message "Jailgun online"
   ```

   The CLI auto-discovers your chat id via `getUpdates` on first run; you can
   pass `--chat-id <id>` to skip discovery, or write `TELEGRAM_CHAT_ID=<id>`
   into `telegram/token.env` alongside the token.
5. To ping on every successful local commit, install the post-commit hook:

   ```bash
   just install-hooks   # or run: git config core.hooksPath ops/git-hooks
   ```

   The hook runs `jailgun notify-commit` after each commit. If `telegram/token.env`
   is missing, the hook exits quietly without failing the commit.

Run history and deploy events also fan out over the dashboard's WebSocket
endpoint regardless of Telegram setup.

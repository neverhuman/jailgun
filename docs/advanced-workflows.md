# Advanced archive and deployment workflows

Archive generation, `jailhard`, tar validation, guarded deployment and optional
notifications remain available. They use the advanced configuration and execution
interfaces. Concept submissions cannot enable deployment or supply arbitrary
bridge commands, environment variables or filesystem paths.

## Configure and inspect

Start from `config/jailgun.example.toml` and save operator overrides in ignored
local configuration. Installed commands resolve the shipped default relative to
the executable; an explicit missing configuration path fails. Inspect the
available options before configuring an archive run:

```bash
jailgun validate-config --config config/jailgun.example.toml
jailgun run --help
jailhard --help
jailgun deploy-archive --help
jailgun remote-cleanup --help
```

The example configuration disables deployment. Keep real prompt files, generated
archives, receipts, browser profiles, remote destinations and credentials outside
committed files. Existing archive defaults can still be relative to the working
directory; set their paths explicitly. See [runtime layout](runtime-layout.md).

For an existing generated archive:

```bash
jailgun tar-validate output.tar.gz --require-single-top-level
```

`jailgun serve --advanced --config CONFIG` starts the advanced service.
`--live` selects its live execution mode. Its dashboard monitor is at
`?advanced=1`; `?advanced=1&demo=1` shows labelled synthetic records.
Execution and arbitrary-path settings require operator authorization. The
compatibility MCP tools are described in [MCP](mcp.md).

Keep the archive scheduler and older binaries stopped before migrating their
accounts into the concept runtime. Archive operations reject profiles already
claimed by the concept scheduler. Preserve account metadata and browser profiles
through migration; do not copy or delete profile locks to work around ownership.

## Deployment boundaries

Deployment is opt-in. Use a dry run and inspect its receipts before enabling a
real remote operation. `preserve-reset` records verified preservation refs and
durable receipts before reset. Dirty checkouts, missing `origin/main`, failed
preservation, changed checkout state and an existing mutation owner stop the
operation. Untracked files are not removed with `git clean`.

Keep unrelated writers out of the deployment checkout. See
[deployment safety](advanced-deploy-safety.md) for the tested destructive
boundaries and rollback behavior. A concept's generated text cannot authorize
these operations.

## Optional Telegram notifications

Notifications are optional. Create a bot using Telegram's normal BotFather
flow, open its chat and send `/start`. Save the bot token in a private local file
with mode 0600, under a directory with mode 0700. Then explicitly select the
token and private chat-ID cache:

```bash
jailgun telegram-send \
  --token-file "$HOME/.jailgun/notifications/telegram/token.env" \
  --chat-id-cache "$HOME/.jailgun/notifications/telegram/chat_id.cache" \
  --message "Jailgun online"
```

The command can discover a chat ID from updates; `--chat-id` selects one
explicitly. Keep token plaintext out of shell command arguments, repository
files and public diagnostics. Deploy notification options are listed in
`jailgun run --help`; commit notifications use `jailgun notify-commit --help`.
Repository hooks are an optional contributor setup, not an installation
requirement. Synthetic browser evidence does not verify Telegram delivery.

## Existing remote CDP helpers

The repository retains `scripts/chrome-cdp-tunnel.sh` and
`scripts/chrome-cdp-launchd.sh` for advanced Chrome setups. Inspect the scripts and
configuration before use; they require operator-managed SSH keys and known-host
verification. Concept workflows use their own local account supervisor and do
not require a CDP tunnel. For a remote concept installation, forward the dashboard
and use [server login](server-login.md).

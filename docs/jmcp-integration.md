# JMCP Integration

Jailgun does not talk to Telegram, Slack, or any user-facing channel
directly. All notifications are routed through the **JMCP outbox** —
jailgun writes JPCM/2.0 envelopes into a directory, and a separate JMCP
bridge picks them up and delivers them.

## Why

- One owner of the user channel (JMCP). Credentials live there only.
- Jailgun has zero network secrets after this change.
- Envelopes queue naturally — JMCP downtime never blocks jailgun.
- Bidirectional reply support is a future bridge feature, not a jailgun
  change.

## The contract

### Outbox location

Default: `~/code/jmcp/inbox`. Override with `JAILGUN_JMCP_INBOX_DIR`
in any env file the runner reads, or `--jmcp-inbox-dir <path>` on the
CLI.

Layout jailgun owns:

```
$JAILGUN_JMCP_INBOX_DIR/
├── tmp/             # mode 0700 — write-then-rename staging
└── jpcm_<id>.json   # mode 0600 — final envelopes the bridge consumes
```

The bridge is expected to move envelopes out of the inbox once they are
delivered (`delivered/`) or failed (`failed/`), but jailgun never reads
those directories.

### Atomic write

Every envelope is written via:

1. `tmp/jpcm_<id>.json.partial` (mode 0600).
2. `flush()` + `sync_all()`.
3. `rename` into `$inbox/jpcm_<id>.json`.

The bridge never sees a half-written file. Crash recovery is a no-op:
any leftover `tmp/*.partial` can be safely deleted by the bridge.

### Envelope shape

The wire format is JPCM/2.0 from
`~/code/jmcp/tips/v6/JPCM_FINAL_PROTOCOL_SCHEMA_v1.0.0.json`.
Jailgun populates the required top-level fields plus a discriminated
`payload`:

```json
{
  "schema_version": "jpcm/1.0.0",
  "envelope_id":    "jpcm_<22-urlsafe-base64>",
  "event_type":     { "domain": "observability", "action": "notification", "risk_tier": "R0_local_read" },
  "event_time":     "2026-06-01T22:33:44Z",
  "producer":       { "name": "jailgun", "version": "0.1.0", "kind": "native_service" },
  "authority":      { "lease_id": "jailgun-v0-pre-lease", "autonomy_tier": "R0_local_read", "granted_by": "jailgun", "granted_at": "..." },
  "task":           { "task_id": "run-A-tab-03", "run_id": "run-A", "tab_id": 3 },
  "routing":        { "stream": "JPCM.USER", "subject": "jpcm.local.dev.user.notification.R0_local_read.jailgun", "destinations": ["user.telegram"] },
  "payload":        { "kind": "jailgun.notify-event", ... },
  "integrity":      { "algo": "sha256", "digest_hex": "<64 hex>" }
}
```

`authority.lease_id = "jailgun-v0-pre-lease"` is the sentinel jailgun
sends today; once JMCP has real lease authority, jailgun acquires a
lease per session and replaces the sentinel.

### Payload variants

```text
kind = jailgun.notify-commit    payload = NotifyCommitPayload
kind = jailgun.notify-event     payload = NotifyEventPayload  (the main one)
kind = jailgun.notify-text      payload = NotifyTextPayload   (ad-hoc CLI text)
```

Every variant carries:

- `title` — short headline.
- `summary_emoji` — single glyph the bridge can render as a reaction
  or message prefix.
- `body_markdown` — the ready-to-send message body (the same text the
  old direct-Telegram path produced).

`NotifyEventPayload` additionally carries `metrics`, a string map the
bridge can render as inline buttons / chips (e.g. `early_stops=3/7`).

### Integrity

`integrity.digest_hex` = `sha256(JSON.serialize(payload))`. The bridge
should re-serialize and re-hash to verify the envelope wasn't tampered
with between write and read.

## Producers in jailgun

| Producer | Trigger | Subcommand / Code path |
|---|---|---|
| Post-commit hook | git commit lands locally | `ops/git-hooks/post-commit` → `jailgun notify-commit` |
| Live run subscriber | `EventKind` in `{PromptSubmitted, DownloadReceipt, DeployFinished}` | `jailgun run --notify-jmcp` / `jailgun serve --live --notify-jmcp` |
| Ad-hoc CLI | manual one-shot | `jailgun jmcp-send --message "..."` |

The subscriber stays event-driven; if the broadcast bus lags or closes,
the subscriber drops gracefully and never blocks the run.

## What jailgun no longer does

- No `reqwest` calls to `api.telegram.org`.
- No bot token reading.
- No chat-id cache.
- No `--notify-telegram` / `--telegram-token-file` /
  `--telegram-chat-id-cache` flags.
- No `telegram/` directory in this repo.

## Bridge expectations (out of scope here)

`~/code/jmcp/bin/jmcp-bridge.mjs` (or any future JMCP implementation)
is expected to:

1. Watch the inbox directory (e.g. `fs.watch`, `inotify`, or a
   `setInterval` poll).
2. For each `jpcm_*.json` file:
   - Read + parse + validate against the v1.0.0 schema.
   - Verify `integrity.digest_hex`.
   - Translate `payload` into the user-facing channel (Telegram, Slack,
     etc.). Render `body_markdown` as the message body.
   - On success, move to `delivered/<envelope_id>.json`.
   - On failure, move to `failed/<envelope_id>.json` with an appended
     `last_error` field. The bridge — not jailgun — owns retry.

The credential for the user-facing channel (e.g. the Telegram bot
token) lives ONLY in `~/code/jmcp/.env`. It is not in jailgun's tree.

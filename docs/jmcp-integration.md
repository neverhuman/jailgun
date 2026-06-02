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
kind = jailgun.batch-request    payload = BatchRequestPayload
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

`BatchRequestPayload` additionally carries:

- `count` - number of child jailgun runs JMCP should fan out after the
  approval gate clears.
- `config_path` - config file path used to build each child
  `jailgun run` invocation.
- `prompt_file` - prompt file used by each child run.
- `child_command` - the child command template JMCP can execute after
  approval. When the operator passes repeated `--profile-pool DIR`
  flags to `jailgun runs`, those flags are preserved in this command so
  every child run uses the same managed Google profile pool.
- `execution_mode` - the default dispatch mode. Jailgun sets this to
  `serial` so the bridge can fan out child runs one at a time unless a
  future operator policy says otherwise.
- `approval_required` - explicit approval gate flag. JMCP should not
  dispatch children until the operator approves the batch.

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
| Batch launch request | operator requests N child runs | `jailgun runs --count N --config ... --prompt-file ...` |
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

## Hardening Tasks

JMCP can request automated codebase hardening runs by delivering a
`jailgun.harden-task` envelope. The flow:

1. **JMCP writes a task envelope** into the jailgun inbox
   (`$JAILGUN_JMCP_INBOX_DIR/jpcm_*.json`). The envelope's `payload`
   contains the repo name, path, CI command, focus area, and the name
   of the prompt template to fill.
2. **Jailgun reads the envelope**, loads the named template from
   `prompts/templates/`, and substitutes the payload fields into the
   template's `{{PLACEHOLDER}}` markers.
3. **Jailgun delivers the filled prompt to jekko** (or runs it
   directly in batch mode). Jekko acts as the execution agent.
4. **The agent executes the hardening**: pulls latest, verifies a
   green CI baseline, makes atomic commits, runs CI after each change,
   and pushes only if everything passes.

### Envelope payload schema

The `payload` for a `jailgun.harden-task` envelope:

| Field | Type | Description |
|---|---|---|
| `kind` | string | Always `"jailgun.harden-task"` |
| `repo_name` | string | Repository name (e.g. `"jailgun"`) |
| `repo_path` | string | Local path (e.g. `"~/code/jailgun"`) |
| `ci_command` | string | CI validation command to run |
| `focus_area` | string | Hardening focus (e.g. `"error handling"`) |
| `template` | string | Template name in `prompts/templates/` (without `.txt`) |

See `contracts/fixtures/jmcp/harden-task.json` for a complete example
envelope.

### How jekko fits

Jekko is the intermediary between jailgun and the agent runtime:

- Jailgun is the **task manager** — it receives envelopes, fills
  templates, and tracks runs.
- Jekko is the **execution agent** — it receives the filled prompt
  and performs the actual git operations, code changes, and CI runs.
- JMCP is the **orchestrator** — it decides when to send harden-task
  envelopes (manually, on a schedule, or in response to events).

For details on the prompt template format and how to create new
templates, see `prompts/templates/README.md`.

## Batch Requests

Batch requests reuse the same outbox and integrity rules as other
JMCP envelopes, but the payload is interpreted as an approval-gated
fan-out request instead of a notification.

1. The operator runs `jailgun runs --count N ...`.
2. Jailgun writes a `jailgun.batch-request` envelope with the child
   command template, config path, prompt path, and
   `approval_required: true`.
3. JMCP creates an approval challenge and holds the batch until the
   operator approves it.
4. After approval, JMCP fans out `N` child work orders by executing the
   stored child command template with distinct child identifiers.
5. Each child run writes its own run evidence and receipts. The cockpit
   uses the event stream plus receipt count to derive batch quality.

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

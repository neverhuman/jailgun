# Standard MCP interface

For the browser-backed trusted-agent fleet, see the dedicated
[Jailgun Worker MCP guide](worker-mcp.md). Worker mode exposes isolated account
profiles, account-bound tabs, generic jobs, model/effort controls and verified
tar.gz transfer through the supervised ChatGPT browser runtime.

The implementation branch supports MCP protocol versions through 2025-11-25
using the official Rust SDK. The daemon owns accounts, scheduling and SQLite
state. Neither transport creates another scheduler. Account login stays in the
operator dashboard; an MCP client cannot submit passwords or verification codes.

Start the development daemon with `jailgun serve` and its installed
runtime assets, or explicit `--assets` and `--node` development paths. Connect
and confirm an account and model before creating an automation token. See
[authentication and token management](authentication-upgrade.md) for issuance,
account scopes and revocation. See [installation](install.md) for the development
build and bundle status.

## Stdio

Configure an MCP client to launch:

```json
{
  "mcpServers": {
    "jailgun": {
      "command": "jailgun",
      "args": ["mcp", "--token-file", "/absolute/path/to/private-token"]
    }
  }
}
```

The token file must be private (0600 on Linux/macOS). Alternatively, supply
`JAILGUN_TOKEN` through the client's secret environment configuration. Do not
commit the credential or pass it as a command argument. Explicit token files take
precedence over `JAILGUN_TOKEN`; without either, a local client uses the runtime's
operator credential. Prefer a scoped automation token for MCP clients. The
gateway uses the same safe daemon startup/reuse as the CLI. `--no-start` disables
startup; an explicit `--url` requires an already running daemon. All protocol
traffic goes to stdout; diagnostics go to stderr.

The default daemon address is `http://127.0.0.1:8787`. `--url` or `JAILGUN_URL`
selects another loopback HTTP origin. A remote daemon uses an SSH local forward;
HTTPS/public listeners are not the supported deployment model.

## Streamable HTTP

Connect an MCP client to `http://127.0.0.1:8787/mcp` with
`Authorization: Bearer <automation-token>`. `x-jailgun-token` remains supported.
Use the client's Streamable HTTP transport, which performs initialization,
version negotiation and notifications. Hand-written single JSON requests do
not verify MCP interoperability. This endpoint is stateless at the transport
level; durable run state and idempotency live in SQLite. The server checks the
credential on every request, including after token revocation, and rejects
foreign Origin and Host headers.

## Tools

Every listed tool publishes Rust-derived input and output JSON schemas.

| Tool | Operation |
| --- | --- |
| `jailgun.brainstorm` | Submit concept text, account ID, 5–10 candidates, optional constraints/criteria, and an idempotency key. |
| `jailgun.run_status` | Read the persisted run, stages, candidates, budgets and artifact metadata. |
| `jailgun.run_result` | Read the completed result manifest and explicit final artifact reference; incomplete runs return an actionable error. |
| `jailgun.run_artifact` | Read a registered text artifact in bounded chunks with integrity metadata. |
| `jailgun.run_pause` | Pause future submissions while safely capturing accepted work. |
| `jailgun.run_resume` | Resume; `allow_incomplete: true` explicitly excludes failed candidates and requires at least three complete candidates. |
| `jailgun.run_cancel` | Persist cancellation and retain results. Repeated cancellation is safe. |
| `jailgun.accounts` | List accounts permitted by this token and their readiness. |
| `jailgun.auth_status` | Read readiness and the operator dashboard action when reconnect is needed. |

Example brainstorm arguments:

```json
{
  "concept": "A neighborhood tool lending library",
  "account_id": "library-account",
  "candidate_count": 5,
  "constraints": "Volunteer operation with a small initial budget",
  "idempotency_key": "library-concept-001"
}
```

Reuse that key only for an identical request. An accepted response includes
`run_id`, `status`, `run_url`, and `result_url`. Status/result/control tools take
`{"run_id":"returned-id"}`; resume optionally adds `allow_incomplete`.
Model judgments are not independently verified evidence. The returned manifest
lists registered artifacts; retrieve them using `jailgun.run_artifact` below or
the authenticated URLs described in the [HTTP reference](concept-http-api.md).

A schema mismatch or unknown tool produces an MCP request error. A valid tool
request that cannot proceed returns `isError: true` with structured `code`,
`message` and `next_action`. Reauthentication does not reset submission budgets
or repeat completed candidates.

## Advanced compatibility

An operator credential additionally exposes `jailgun.run`
and `jailgun.run_summary`. The former retains archive request settings and the
`account` alias. `jailgun.run_status` also reads archive snapshots for operators.
Archive execution cannot run against a concept-owned runtime. Automation tokens
cannot list or execute these advanced tools. `jailgun.submit_code` was removed;
finish website login in the operator-controlled browser instead.

## Verification and limits

The Rust tests connect an official SDK client to the actual HTTP listener and
exercise negotiation, schemas, notifications, malformed requests, account scopes,
revocation, Origin rejection and archive compatibility. The browser E2E lane
also exercises five/ten-candidate HTTP and stdio MCP workflows through the real
Chrome adapter against synthetic ChatGPT. Synthetic evidence does not establish
live ChatGPT acceptance; live Linux/macOS verification remains a release gate.


## Read result text

`jailgun.run_result` returns `final_artifact`, the registered accepted revision
artifact. Other candidates, comparison, critique and manifests remain available
in the run's artifact list, including explicitly marked partial attempts.
Call `jailgun.run_artifact` using registered IDs, never a filesystem path:

```json
{"run_id":"RUN_ID","artifact_id":"ARTIFACT_ID","offset":0,"limit":16384}
```

The response contains `text`, `offset`, `next_offset`, `eof`, and `artifact`
metadata (including `completion`, full `byte_length` and full `sha256`). Offsets
and limits count UTF-8 bytes. Each chunk ends on a character boundary. Continue
with exactly `next_offset` until `eof` is true; concatenate UTF-8 bytes and verify
the full length and SHA-256 before accepting the download. Limits must be between
4 and 16384 bytes. At the exact end, reads return empty text with `eof: true`.

Every chunk rechecks the caller's account authorization and token revocation,
then verifies the complete stored artifact against its registered digest.
Corruption fails the read. An artifact ID from another run or an arbitrary path
is rejected. A partial artifact remains partial even after its final chunk.
These operations are available through both the HTTP transport and stdio gateway.

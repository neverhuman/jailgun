# Jailgun Worker MCP

Jailgun Worker is the small, browser-free execution mode for trusted agents. It
runs as one Rust process, listens only on loopback, stores each logical tab in a
private workspace, and exposes standard Streamable HTTP MCP. A remote client
reaches it through an SSH local forward, so the bearer credential and MCP traffic
stay inside the encrypted SSH channel.

One daemon can host several authenticated AI accounts. Each account is an
independent logical worker with its own private `CODEX_HOME`; every tab is pinned
to exactly one account for its lifetime. Operators who need stronger OS or
resource isolation can instead run one daemon per account with distinct runtime
directories and loopback ports.

Worker mode does not start Chrome, the dashboard, SQLite, or the concept
scheduler. It invokes `codex exec` directly without a shell and uses
`workspace-write` sandboxing. Worker tools require the operator credential
because submitted jobs can modify files and run tools inside their tab workspace.

## Start the worker

```bash
jailgun worker \
  --runtime "$HOME/.jailgun-worker" \
  --addr 127.0.0.1:8790
```

First start creates a random 64-character credential at
`~/.jailgun-worker/operator-token` with mode 0600. Runtime directories use mode
0700. The server refuses non-loopback listeners. Set `JAILGUN_CODEX_BIN` only
when `codex` is not on the service PATH.

Health check from the worker host:

```bash
curl --fail http://127.0.0.1:8790/api/health
```

## Account profiles and login

Register a profile with `jailgun.worker.account_register` before opening a tab:

```json
{
  "account_id": "team-primary",
  "label": "Team primary ChatGPT account"
}
```

Registration creates
`~/.jailgun-worker/worker/accounts/team-primary/codex` with mode 0700. It does
not accept or return a password, API key, access token, cookie, or browser
session. On the worker host, log in directly with Codex:

```bash
CODEX_HOME="$HOME/.jailgun-worker/worker/accounts/team-primary/codex" \
  codex login --device-auth
```

Codex owns its credential-file format. Jailgun supplies the isolated
`CODEX_HOME`, keeps account metadata at mode 0600, and removes ambient
`OPENAI_API_KEY` and `CODEX_ACCESS_TOKEN` from job processes so one profile
cannot silently borrow another credential. Keep the runtime on an encrypted
volume when credentials or job data require encryption at rest. Never copy its
account directory into a repository or tar object.

After login, call `jailgun.worker.account_refresh`. It runs `codex login status`
inside only that profile and reports `ready`, `login-required`, or `unavailable`.
`jailgun.worker.account_list` refreshes every profile by default and returns
`total`, `ready`, `login_required`, and `unavailable` counts. A job-side 401 or
equivalent authentication failure also marks that account `login-required`.
No credential material appears in MCP responses or logs.

Add each additional account with a distinct ID and repeat login. When exactly
one account is ready, `account_id` may be omitted. When several are ready, the
agent must choose an account explicitly. This prevents nondeterministic account
selection and cross-account tab reuse.

## Remote encrypted client

On the client machine, create an SSH tunnel:

```bash
ssh -N \
  -o ExitOnForwardFailure=yes \
  -L 127.0.0.1:8790:127.0.0.1:8790 \
  worker-host
```

Copy the credential once over SSH, store it outside repositories, and restrict
it to the current user:

```bash
install -d -m 700 "$HOME/.config/jailgun"
scp worker-host:.jailgun-worker/operator-token \
  "$HOME/.config/jailgun/worker.token"
chmod 600 "$HOME/.config/jailgun/worker.token"
```

Configure any stdio MCP client to use Jailgun's gateway:

```json
{
  "mcpServers": {
    "jailgun-worker": {
      "command": "jailgun",
      "args": [
        "mcp",
        "--no-start",
        "--url", "http://127.0.0.1:8790",
        "--token-file", "/absolute/private/path/worker.token"
      ]
    }
  }
}
```

Streamable HTTP clients may connect directly to
`http://127.0.0.1:8790/mcp` through the tunnel and send
`Authorization: Bearer <token>`. Use MCP protocol negotiation; do not put the
token in a URL or command argument.

## Tool map

| Tool | Purpose |
| --- | --- |
| `jailgun.worker.info` | Limits, executor identity, model aliases, efforts, cached account counts. |
| `jailgun.worker.account_register` | Create an isolated account profile; never accepts credentials. |
| `jailgun.worker.account_list` | Refresh/list profiles and ready/relogin/unavailable counts. |
| `jailgun.worker.account_refresh` | Recheck one profile with `codex login status`. |
| `jailgun.worker.tab_open` | Create an account-bound workspace; optionally expand one verified input object. |
| `jailgun.worker.tab_list` | List open/closed tabs, defaults, and active job ownership. |
| `jailgun.worker.tab_set_model` | Change model and reasoning effort on an idle tab. |
| `jailgun.worker.tab_close` | Close an idle tab, or request cancellation with `force:true`. |
| `jailgun.worker.job_submit` | Submit one idempotent job with timeout and error-reporting policy. |
| `jailgun.worker.job_status` | Poll durable-in-process status and obtain the output object ID. |
| `jailgun.worker.job_cancel` | Request idempotent cancellation. |
| `jailgun.worker.object_put` | Upload a tar.gz in ordered, verified base64 chunks. |
| `jailgun.worker.object_get` | Download a tar.gz in bounded base64 chunks. |
| `jailgun.worker.object_list` | List verified input/output objects and SHA-256 receipts. |

`astra` maps to `gpt-6-astra`; `sol` maps to `gpt-5.6-sol`. Explicit safe model
IDs are accepted for forward compatibility. Reasoning efforts are `low`,
`medium`, `high`, `xhigh`, `max`, and `ultra`. Call `jailgun.worker.info` instead
of hard-coding limits.

## End-to-end agent flow

1. Call `jailgun.worker.info`, then `account_list`.
2. If no suitable account is ready, register it; the host operator logs in and
   the agent calls `account_refresh`.
3. If work has source files, upload its tar.gz using `object_put` chunks.
4. Open a tab with `account_id`, the returned `object_id`, model, and effort.
5. Submit a job with a unique `idempotency_key`.
6. Poll `job_status` until a terminal state.
7. On `completed`, download `output_object_id` with `object_get` and verify the
   full byte count and SHA-256.
8. Close the tab.

Open a tab:

```json
{
  "tab_id": "api-refactor",
  "account_id": "team-primary",
  "model": "astra",
  "reasoning_effort": "xhigh",
  "input_object_id": "object-0123456789abcdef"
}
```

Submit work:

```json
{
  "tab_id": "api-refactor",
  "prompt": "Run tests, repair the API, and leave a concise result in jailgun-result.md.",
  "model": "sol",
  "reasoning_effort": "high",
  "timeout_seconds": 3600,
  "error_reporting": "detailed",
  "idempotency_key": "job-001"
}
```

Repeating the exact request with the same idempotency key returns the original
job. Reusing the key with different settings fails. A tab owns at most one
active job. Model changes on busy or closed tabs fail.

## Tar object transfer

Only non-empty `.tar.gz` objects are accepted. The complete compressed object
is capped at the size returned by `worker.info`. Archives containing absolute
paths, parent traversal, `.git`, links, devices, or special entries are rejected
before extraction. Expanded files have a second size cap. Output archives skip
symlinks and `.git`.

For each upload:

- Compute SHA-256 over the complete compressed tar.gz bytes.
- Send standard base64 chunks no larger than `max_chunk_bytes` decoded.
- First call omits `upload_id` and uses offset 0.
- Continue with returned `upload_id` and `next_offset`.
- Set `final_chunk:true` only on the last chunk.
- Keep `name`, `total_bytes`, and `sha256` identical on every call.

First upload chunk:

```json
{
  "name": "source.tar.gz",
  "offset": 0,
  "data_base64": "H4sI...",
  "final_chunk": false,
  "total_bytes": 812345,
  "sha256": "64-lowercase-or-uppercase-hex-characters"
}
```

Download chunks with:

```json
{
  "object_id": "object-0123456789abcdef",
  "offset": 0,
  "limit": 65536
}
```

Decode and append `data_base64`; continue at `next_offset` until `eof:true`.
Verify the assembled compressed bytes against `object.size_bytes` and
`object.sha256` before extracting.

## Status, timeout, errors, and cancellation

Job states are `queued`, `running`, `cancelling`, `completed`, `failed`,
`timed-out`, and `cancelled`. `timeout_seconds` includes executor runtime and is
bounded to 24 hours. Timeout and cancellation kill the child process; terminal
calls are idempotent.

`error_reporting` controls status detail:

- `off`: expose state without executor text.
- `summary`: retain up to the final 4 KiB of executor diagnostics.
- `detailed`: retain up to the final 64 KiB.

Diagnostics and prompts remain beneath the private worker runtime and are never
part of committed repository fixtures. Output tar objects are authenticated but
not encrypted at rest; rely on private filesystem permissions or encrypted
storage when required.

## Mock CI contract

`bash ops/ci/worker.sh` runs worker unit tests and official-SDK MCP integration
tests with an injected fake executor. It never invokes `codex`, Chrome, a live
model, external setup, or real credentials. Mock coverage includes multiple
account routing, readiness counts, credential-directory permissions, models,
timeouts, cancellation, transfers, and the official MCP SDK. Full CI also runs
Rust formatting, Clippy, workspace tests, security scans, and the Jankurai audit.

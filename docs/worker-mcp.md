# Jailgun Browser Worker Fleet MCP

Jailgun exposes authenticated ChatGPT website sessions as a small, loopback-only
MCP worker fleet. One Rust daemon supervises one private Chrome profile per
authenticated account. Agents can select an account, open a logical tab, choose
a model and reasoning effort, submit a job, poll status, cancel work, and move
verified `.tar.gz` objects in both directions.

The production worker does **not** use `codex login`, device codes, API keys, or
`codex exec`. Account owners sign in through the normal ChatGPT website inside
Jailgun's private browser viewer. CI uses injected mocks and never contacts a
live account.

## Security and ownership model

- The daemon listens only on loopback. Remote clients must use an SSH local
  forward or another operator-approved encrypted tunnel.
- MCP worker tools require the operator bearer token. Keep its file mode `0600`.
- Each account has a separate Chrome profile below the private runtime. Runtime
  directories use mode `0700`; the daemon never returns cookies or credentials.
- A logical tab is permanently pinned to one account. It cannot switch accounts.
- Passwords, MFA codes, and Cloudflare challenges are completed only in the
  private login viewer. Never send them through MCP, prompts, logs, or chat.
- SSH encrypts token and MCP traffic in transit. For encryption at rest, place
  the runtime on an encrypted host volume; filesystem permissions are not disk
  encryption.
- Every input/output object has a byte length and SHA-256 receipt. Unsafe tar
  paths, links, devices, `.git`, and oversized archives are rejected.

## Runtime shape

`jailgun worker` starts the browser-backed dashboard and worker MCP surface. It
defaults to port `8790`. `jailgun serve` also exposes the same worker tools on
its configured port, normally `8787`, alongside concept workflows.

```bash
jailgun worker \
  --runtime "$HOME/.jailgun" \
  --addr 127.0.0.1:8790 \
  --server-browser
```

For a workstation with local Chrome, omit `--server-browser`. For a Linux host,
run the same prerequisite check first:

```bash
jailgun doctor --runtime "$HOME/.jailgun" --server-browser
```

On first start Jailgun creates `operator-token` beneath the runtime with mode
`0600`. The service refuses a non-loopback listener.

Health check:

```bash
curl --fail http://127.0.0.1:8790/api/health
```

## Authenticate accounts

1. Open the operator dashboard and go to **Accounts**.
2. Create or reconnect an account.
3. Open its private login viewer.
4. Complete normal ChatGPT website login and MFA in that viewer.
5. Confirm the detected email address.
6. Leave the account in `ready` state.

Repeat for every account. Use distinct, stable account IDs such as
`ben-veox-ai` or `klara-jonsson-data-gmail-com`. The dashboard owns account
registration; `jailgun.worker.account_register` exists for compatibility with
older clients and returns `dashboard-account-required` in browser-worker mode.

`jailgun.worker.account_list` returns:

- `counts.total`
- `counts.ready`
- `counts.login_required`
- `counts.unavailable`
- each account's ID, label, status, active tab count, selected model, and models
  observed from that account's ChatGPT UI

Use `jailgun.worker.account_refresh` to recheck one account. A failed identity
probe or expired ChatGPT session moves the account out of `ready`; reconnect it
in the dashboard. Account metadata is visible to the worker, but authentication
material is not.

## Remote encrypted access

Forward the worker port from the client machine:

```bash
ssh -N \
  -o ExitOnForwardFailure=yes \
  -L 127.0.0.1:18790:127.0.0.1:8790 \
  worker-host
```

Copy the operator token once through SSH and protect it:

```bash
install -d -m 700 "$HOME/.config/jailgun"
scp worker-host:.jailgun/operator-token "$HOME/.config/jailgun/worker.token"
chmod 600 "$HOME/.config/jailgun/worker.token"
```

Do not put the token in a URL, shell history, repository, prompt, or MCP tool
argument.

## Configure an MCP client

The recommended client transport is Jailgun's stdio gateway. It keeps protocol
messages on stdout and diagnostics on stderr:

```json
{
  "mcpServers": {
    "jailgun-fleet": {
      "command": "jailgun",
      "args": [
        "mcp",
        "--no-start",
        "--url", "http://127.0.0.1:18790",
        "--token-file", "/absolute/private/path/worker.token"
      ]
    }
  }
}
```

Native Streamable HTTP clients may connect to
`http://127.0.0.1:18790/mcp` with `Authorization: Bearer <token>`. Let the MCP
SDK perform initialization and protocol negotiation. Do not treat a plain HTTP
`GET` as an MCP interoperability test.

## Tool catalog

| Tool | Use |
| --- | --- |
| `jailgun.worker.info` | Read interface version, browser executor, aliases, efforts, limits, and readiness counts. |
| `jailgun.worker.account_list` | Refresh and list all dashboard-managed account workers. |
| `jailgun.worker.account_refresh` | Recheck one account's supervised browser session. |
| `jailgun.worker.account_register` | Compatibility tool; browser mode directs the operator to the dashboard. |
| `jailgun.worker.tab_open` | Open one account-bound logical tab, optionally with an input object. |
| `jailgun.worker.tab_list` | List tab state, ownership, defaults, and active jobs. |
| `jailgun.worker.tab_set_model` | Change model and reasoning effort on an idle open tab. |
| `jailgun.worker.tab_close` | Close an idle tab; `force:true` first requests cancellation. |
| `jailgun.worker.job_submit` | Submit one idempotent browser job. |
| `jailgun.worker.job_status` | Read job state, errors, timestamps, and output object ID. |
| `jailgun.worker.job_cancel` | Request idempotent cancellation. |
| `jailgun.worker.object_put` | Upload a verified `.tar.gz` in ordered base64 chunks. |
| `jailgun.worker.object_get` | Download a verified input/output object in bounded chunks. |
| `jailgun.worker.object_list` | List object names, kinds, sizes, media types, and SHA-256 receipts. |

Call `jailgun.worker.info` at session start instead of hard-coding limits.

## Minimal one-tab job

Call `jailgun.worker.info` with `{}`, then refresh account readiness with
`jailgun.worker.account_list`:

```json
{
  "refresh": true
}
```

Open a tab on a specific ready account:

```json
{
  "tab_id": "proof-ben-001",
  "account_id": "ben-veox-ai",
  "model": "current",
  "reasoning_effort": "medium"
}
```

Submit one harmless test job:

```json
{
  "tab_id": "proof-ben-001",
  "prompt": "Reply with exactly: JAILGUN_OK ben-veox-ai",
  "timeout_seconds": 120,
  "error_reporting": "detailed",
  "idempotency_key": "proof-ben-001-2026-10-01"
}
```

Poll `jailgun.worker.job_status` using the returned `job_id`:

```json
{
  "job_id": "job-returned-by-submit"
}
```

Terminal states are `completed`, `failed`, `timed-out`, and `cancelled`.
Intermediate states are `queued`, `running`, and `cancelling`. On completion,
download `output_object_id`; the archive contains `jailgun-result.md`. Verify the
full archive size and SHA-256 before reading it.

Close with `jailgun.worker.tab_close`:

```json
{
  "tab_id": "proof-ben-001",
  "force": false
}
```

Repeat for each account with a unique tab ID and idempotency key. This proves
routing only when the response is captured, the output hash verifies, and the
job's `account_id` equals the requested account.

## Models and reasoning effort

Portable model values:

- `current`: keep that account's current ChatGPT selection
- `astra`: select the one observed model label containing the standalone word `Astra`
- `sol`: select the one observed model label containing the standalone word `Sol` (for example, `GPT-5.6 Sol`)
- an exact model label reported in `available_models`

Reasoning efforts are `low`, `medium`, `high`, `xhigh`, `max`, and `ultra`.
Availability is model- and account-dependent. `medium` may use the provider
default when ChatGPT shows no selector. An alias fails if zero or multiple
observed labels match. Any other unavailable effort or model fails explicitly;
Jailgun never silently substitutes a different choice.

Change an idle tab before its next job:

```json
{
  "tab_id": "analysis-klara-001",
  "model": "astra",
  "reasoning_effort": "xhigh"
}
```

One account may own up to the browser capacity reported by the service. One
logical tab has at most one active job. Separate accounts can run concurrently.

## Upload a `.tar.gz` input object

Only non-empty `.tar.gz` basenames are accepted. Compute SHA-256 over the entire
compressed file. Split it into chunks no larger than `max_chunk_bytes` after
base64 decoding.

First `jailgun.worker.object_put` call:

```json
{
  "name": "source.tar.gz",
  "offset": 0,
  "data_base64": "H4sI...",
  "final_chunk": false,
  "total_bytes": 812345,
  "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
}
```

Continue with returned `upload_id` and `next_offset`. Keep `name`, `total_bytes`,
and `sha256` identical:

```json
{
  "upload_id": "upload-returned-by-first-call",
  "name": "source.tar.gz",
  "offset": 262144,
  "data_base64": "...",
  "final_chunk": true,
  "total_bytes": 812345,
  "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
}
```

The final call verifies size, SHA-256, gzip/tar structure, and paths before it
returns `object_id`. Open a new tab with that object:

```json
{
  "account_id": "bentaylorche-gmail-com",
  "model": "sol",
  "reasoning_effort": "high",
  "input_object_id": "object-returned-by-upload"
}
```

Jailgun expands the object into that tab's private workspace and attaches the
verified archive to the browser job. `input_object_id` can be supplied only when
opening a new tab, not when reusing an existing tab.

## Download output objects

Start `jailgun.worker.object_get` at offset zero:

```json
{
  "object_id": "object-returned-by-job-status",
  "offset": 0,
  "limit": 65536
}
```

Decode and append `data_base64`. Continue at `next_offset` until `eof:true`.
Then verify:

1. assembled byte length equals `object.size_bytes`;
2. SHA-256 equals `object.sha256`;
3. the archive passes safe tar validation;
4. `jailgun-result.md` contains the expected captured response.

## Idempotency, timeout, errors, and cancellation

Every job requires an `idempotency_key` of 1–128 bytes. Repeating the exact
request returns the original job. Reusing a key with different input returns
`idempotency-conflict` and does not submit another ChatGPT turn.

`timeout_seconds` must be within `worker.info` limits. On timeout, Jailgun asks
the owned browser conversation to stop, captures partial output when possible,
and reports `timed-out`. Cancellation uses the same ownership checks and cannot
stop another tab's conversation.

`error_reporting` controls the job error field:

- `off`: terminal state only;
- `summary`: concise actionable failure;
- `detailed`: full bounded browser failure and next action.

| Code | Meaning | Action |
| --- | --- | --- |
| `account-required` | Several accounts are ready and none was selected. | Pass `account_id`. |
| `account-login-required` | Session missing, expired, or unconfirmed. | Reconnect in Accounts. |
| `account-tab-mismatch` | Selected tab belongs to another account. | Use its account or open a tab. |
| `tab-busy` | Tab has active work or is closed. | Wait, cancel, or open another tab. |
| `model-unavailable` | UI does not expose the model. | Use `current` or an observed label. |
| `reasoning-effort-unavailable` | Model has no matching effort. | Use `medium` or another model. |
| `idempotency-conflict` | Key was reused with different settings. | Restore input or use a new key. |
| `unsafe-archive` | Archive failed structure/path checks. | Rebuild with safe regular files. |
| `authentication-expired` | ChatGPT session expired during work. | Reconnect and submit a new job. |

Cancel with `jailgun.worker.job_cancel`:

```json
{
  "job_id": "job-to-cancel"
}
```

Repeated cancellation of a terminal job is safe and returns current state.

## Fleet routing pattern

1. Call `account_list` with `refresh:true`.
2. Filter to `status == "ready"`.
3. Choose explicitly by account ID; never depend on list order.
4. Respect `active_tabs`, account capacity, cooldown, and model list.
5. Use one unique idempotency namespace per caller and account.
6. Record job ID, tab ID, account ID, timestamps, output object ID, size, and
   SHA-256 as the execution receipt.
7. Download and verify output before closing the tab.
8. Reconnect only the affected account when login expires; other workers remain
   usable.

For stronger resource or trust isolation, run one daemon per account with a
separate runtime and loopback port. Never point two daemons at the same runtime
or Chrome profile.

## Service operation

A typical user systemd unit runs the installed binary, uses one private runtime,
and restarts on failure. Keep the listener on `127.0.0.1`. After upgrades:

```bash
systemctl --user is-active jailgun
curl --fail http://127.0.0.1:8790/api/health
jailgun doctor --runtime "$HOME/.jailgun" --server-browser
```

Then call `worker.info`, `account_list`, and one harmless one-tab proof per
account. Jailgun removes only proven-stale Chrome singleton locks; live locks
remain protected.

## Troubleshooting

- **Dashboard pairing expired:** generate a fresh pairing link/code; it is
  intentionally one-use and short-lived.
- **Cloudflare challenge loops:** keep the same private profile, use the normal
  interactive viewer, avoid repeatedly creating profiles, and verify host time,
  DNS, and egress reputation. Jailgun does not bypass challenges.
- **`profile-locked`:** another live Chrome owns the profile. Stop the owning
  service cleanly. Do not delete locks while that process is alive.
- **Signed in but login required:** focus the private viewer, ensure it is on
  `chatgpt.com`, refresh, and confirm the email matches the registered identity.
- **MCP authentication fails:** verify tunnel, token file mode, URL, and Bearer
  authentication through the MCP transport.
- **Client sees old tools:** restart its stdio gateway after daemon upgrade.
- **Completed job has no object:** treat it as failed output packaging; never
  claim success without a verified output receipt.

## Mock CI contract

CI must never open a real provider, use a real profile, or require login.

```bash
bash ops/ci/worker.sh
```

That lane uses injected Rust mock executors and the official MCP SDK. It covers
multi-account routing, readiness, tabs, model/effort settings, idempotency,
timeouts, cancellation, safe archives, chunk transfer, output receipts, and MCP
schemas. Browser bridge tests use loopback fixtures. Full CI also runs formatting,
Clippy, workspace tests, scanners, documentation checks, bundle tests, and the
Jankurai audit.

Live one-tab account proofs are operator acceptance tests. Run them manually
after deployment, use harmless prompts, save only non-secret receipts, and never
add them to CI.

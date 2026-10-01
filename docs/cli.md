# Concept command line

These commands use the same authenticated Rust service and durable workflow as
HTTP and MCP. They are implemented on the v0.2.0 development branch; public
packages and live-provider acceptance are still pending. Account onboarding is
available in the dashboard, including a [private Linux server viewer](server-login.md).

## Connect to the service

`jailgun serve` runs the concept daemon in the foreground. `jailgun setup` starts
it when needed and opens a one-use dashboard pairing link. `--no-open` prints the
link without launching a browser. It expires after five minutes; requesting a
new link invalidates the previous one. The browser exchanges the code for an
HttpOnly session cookie and removes the code from its address/history entry.
Passwords and verification codes belong in ChatGPT's browser login page.

Concept commands reuse the daemon recorded in `~/.jailgun/daemon.json`, or start
one under the installation ownership lock. Concurrent clients cannot open two
schedulers for the same runtime. Background startup diagnostics go to the
private runtime's `daemon.log`. Interrupting a waiting CLI does not cancel its
run; use the explicit cancel command.

```bash
jailgun service status
jailgun service stop
```

These operator commands never start a daemon. Local `stop` verifies the
authenticated instance and runtime, requests graceful shutdown, and waits up to
30 seconds for the ownership lock to be released. An already stopped local
runtime succeeds without creating files. Shutdown retains profiles and completed
results; active attempts keep their persisted acceptance or reconciliation state.
Finish or pause work first when possible. With an explicit `--url` (including an
SSH forward), success reports `stopping`: it acknowledges the request but cannot
verify the remote filesystem lock. Run the local command on the server before
backup or upgrade. Automation tokens cannot inspect or stop the service.

`jailgun service unit --out FILE` writes a new `.service` or `.plist` file for
optional native startup. It requires a stopped local runtime and complete
runtime assets. `--format systemd|launchd` and `--addr` override the host format
and saved loopback port; browser mode and explicit prerequisite paths are
preserved. The JSON response includes `activated: false` and reviewable
activation commands. Follow the [startup guide](startup.md) to activate,
restart, disable or unload it.

For a source checkout with built assets:

```bash
jailgun setup --assets "$PWD" --node "$(command -v node)"
jailgun doctor --assets "$PWD" --node "$(command -v node)"
```

Use the pinned Node version from `.node-version`. Installed bundles will resolve
assets relative to the executable. `doctor` checks the bridge, dashboard, Node,
Chrome and interactive display without creating a runtime or starting a daemon.
A successful prerequisite check does not verify ChatGPT compatibility.
`--headless` supports an already authenticated profile; it cannot provide the
first interactive login by itself.

Connection options accepted by concept commands:

| Option | Meaning |
| --- | --- |
| `--runtime PATH` / `JAILGUN_RUNTIME` | Private local runtime; default `~/.jailgun` |
| `--url ORIGIN` / `JAILGUN_URL` | Explicit loopback HTTP service or SSH forward; never starts a daemon |
| `--token-file PATH` / `JAILGUN_TOKEN_FILE` | Private credential file, regular file with owner-only permissions |
| `--no-start` | Report unavailable service without starting it |
| `--assets PATH`, `--node PATH`, `--chrome PATH` | Explicit local runtime prerequisites |
| `--server-browser` | Linux managed display and operator-only dashboard login viewer; conflicts with `--headless` |
| `--json` | JSON success on stdout; structured errors on stderr |

Credential precedence is explicit token file, `JAILGUN_TOKEN`, then the runtime's
operator credential. Automation tokens retain their account scope and cannot
issue pairing codes or start account login. Tokens are never placed in URLs or
command arguments. Only loopback origins are accepted; use SSH forwarding for a
remote service. For example, with a server listening on its default port:

```bash
ssh -N -L 8787:127.0.0.1:8787 user@server
```

Then connect to `http://127.0.0.1:8787` using the matching private credential.
The CLI will not start a local replacement when an explicit URL is unavailable.

## Accounts and concepts

```bash
jailgun accounts list
jailgun accounts connect
jailgun accounts connect --email person@example.com --id personal
jailgun accounts reconnect personal
jailgun brainstorm "A neighborhood tool library" --account personal --tabs 8
jailgun brainstorm --concept-file concept.md --account personal --tabs 5 --wait
```

`accounts connect` opens onboarding; supplying an email also registers/starts the
account. Login, detected identity confirmation and model selection are operator
operations. Existing bindings, profiles and capacity limits are retained.

A submission sends concept file **contents**, not a path for the server to read.
The default is five perspectives; select five through ten. Add `--constraints`
and repeated `--criterion 'Usefulness=60' --criterion 'Feasibility=40'` to replace
the default criteria. Positive integer weights must total 100.

Without `--wait`, success returns a run ID (or the acceptance object with
`--json`). With `--wait`, success requires a completed final result. The default
output is final Markdown; JSON output is the result manifest. Paused, failed,
cancelled and reconciliation-required runs return a nonzero exit status with an
actionable error. Completed candidates are retained.

For retriable transport failures, retain the reported idempotency key and inspect
`runs list` before retrying. Reuse `--idempotency-key KEY` only with identical
content and options. Changed content returns `idempotency-conflict`. Specific
server errors such as model selection or account authorization are preserved.

## Inspect, control and export

```bash
jailgun runs list
jailgun runs show RUN_ID
jailgun runs result RUN_ID
jailgun runs pause RUN_ID
jailgun runs resume RUN_ID
jailgun runs resume RUN_ID --allow-incomplete
jailgun runs cancel RUN_ID
jailgun runs export RUN_ID --out "results/my concept"
```

Continuing with an incomplete subset requires the explicit option and at least
three completed candidates with no active submissions. Cancellation is
persistent and idempotent. `result` requires completion; `show` and `export`
include retained partial work.

Export destinations must be new or empty. Downloads must match their registered
run, artifact ID, byte count and SHA-256. Files are private and written atomically
without overwriting. `export.json` records the run and artifact paths; only a
completed run receives a root `final.md`. A failed export may retain verified
files and an `.export-in-progress` marker; choose a new directory for retry.
Partial text is never relabeled as a final concept.

## Offline backup and restore

```bash
jailgun service stop --runtime "$HOME/.jailgun"
jailgun data backup --runtime "$HOME/.jailgun" --out "$HOME/jailgun-backups/before-upgrade"
jailgun data restore --backup "$HOME/jailgun-backups/before-upgrade" --out "$HOME/.jailgun-restored"
```

Create the private parent directory first and stop the daemon. Both commands
require a new destination and support `--json`; they do not connect to or start
a daemon. Backup uses the original binary with its matching schema. Restore
preserves account IDs, profile references and retained results, but does not
copy profiles or the operator credential. Follow the full
[backup and rollback procedure](../db/README.md) before restarting.

## Advanced compatibility

`jailgun uninstall [--prefix PREFIX] [--json]` removes verified managed
application files after all processes using that installation have stopped.
Accounts and results are retained. Disable optional startup jobs first and
follow the [removal and interruption procedure](install.md#remove-application-files).
An explicit prefix permits resuming from a verified archive when an interrupted
removal has already deleted the installed executable.

The archive service uses `jailgun serve --advanced --config CONFIG`; the existing
`--live` switch also selects that service. `--advanced --live` is supported.
`--concepts` remains an explicit alias for the default concept service.
Archive configuration, deployment notifications and concept runtime options
cannot be mixed silently. Other archive/deploy and `jailhard` commands remain
available. Concept submissions do not enable deployment.

For standard MCP clients, see [MCP configuration](mcp.md). For persistence and
operator credentials, see [database operations](../db/README.md) and
[authentication changes](authentication-upgrade.md).

The [concept dashboard guide](dashboard.md) covers onboarding, draft recovery,
result reading and operator-managed automation tokens.

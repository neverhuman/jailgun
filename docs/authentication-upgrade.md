# Authentication upgrade (v0.2.0 work in progress)

All private REST endpoints, MCP requests, and WebSocket connections now require
authentication. Operator credentials and paired browser sessions have full access;
revocable automation credentials have explicit account scopes. `/api/health` only returns `{"status":"ok"}`. Static
application assets contain no account or run data. Explicit `?demo=1` uses
synthetic data; failed API requests never switch to demo mode.

`jailgun setup` starts or connects to the local concept daemon, reuses its private
operator credential (`~/.jailgun/operator-token` by default), and explicitly requests
a one-use dashboard pairing link. The link expires after five minutes. Opening it
exchanges the code for an HttpOnly, SameSite=Strict cookie lasting up to eight
hours, then removes the code from the URL. The daemon does not issue or log pairing
secrets on startup. Treat the setup link as a credential; do not capture setup
output in shared logs. Cookies are held in daemon memory; restarting the service
requires dashboard pairing again, independently of the persistent ChatGPT profile.

The daemon holds an ownership lock in the same runtime directory. A second
service using that installation fails with `daemon-already-owned`. `jailgun serve`
starts the concept supervisor and reserves migrated profiles for the durable
scheduler; archive commands refuse those profiles. `--advanced` and the compatible
`--live` option select the separate archive service.

Reconnect verifies the account again and refreshes model discovery, including when
the account was ready. Repeated setup preserves an existing ready session. Login,
confirmation, cancellation and readiness checks run in order per account; an action
may wait for a bounded browser check to finish. Completed runs and profile/port
allocations remain intact. Cooldowns and explicit operator pauses are still enforced.

The service binds to loopback and validates Host and Origin headers. Remote use
requires an SSH forward, for example:

```sh
ssh -NT -L 8787:127.0.0.1:8787 user@server
```

The local concept CLI and stdio MCP gateway ignore HTTP proxy environment settings
and refuse redirects. Their credentials stay on the selected loopback connection;
use SSH forwarding for a daemon on another machine.

Then open `http://127.0.0.1:8787` locally. Installed bundles resolve their assets
without a source checkout; see [installation](install.md) and
[server login](server-login.md) for the tested paths and remaining acceptance limits.

The advanced service uses the same pairing API but does not use concept setup.
With its operator credential, request `POST /api/session/pairing-code`, then enter
the returned `code` at `http://127.0.0.1:8787/?advanced=1`. For example, this explicit
operator command reads the credential privately and prints only the short-lived code:

```sh
python3 - <<'PY'
import json
from pathlib import Path
from urllib.request import HTTPRedirectHandler, ProxyHandler, Request, build_opener
class DirectOnly(HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None
opener = build_opener(ProxyHandler({}), DirectOnly())
token = (Path.home() / '.jailgun/operator-token').read_text().strip()
request = Request('http://127.0.0.1:8787/api/session/pairing-code', data=b'',
                  headers={'Authorization': 'Bearer ' + token}, method='POST')
with opener.open(request, timeout=10) as response:
    print(json.load(response)['code'])
PY
```

For a custom advanced browser registry, use the operator-token file beside that
registry. Do not paste the long-lived operator credential into the dashboard.

For command-line HTTP clients, use `Authorization: Bearer <operator-token>` or
the compatible `x-jailgun-token` header. Explicit `--ingest-token` /
`JAILGUN_INGEST_TOKEN` values must be random, at least 32 characters, and cannot
be example placeholders. Omitting the option generates a credential. Do not put
credentials in URLs or committed configuration. Query-string WebSocket tokens
are no longer accepted; the dashboard uses its cookie. Browser mutations and
cookie WebSocket upgrades require the matching Origin header.

To rotate a generated operator credential, run `jailgun service stop`, move the old token
file into private backup custody, then restart to generate a new credential.
Stopping the daemon invalidates dashboard sessions. Do not replace or delete a
browser profile to rotate application access credentials.

The concept daemon supports separately revocable automation tokens. An operator
creates one with `POST /api/tokens` and JSON `{ "name": "my client",
"account_ids": ["my-account"] }`. The response contains metadata and a `secret`
returned only once. Keep it in the client's private credential store; the database
retains its SHA-256 digest. Future accounts are never added implicitly.

Automation tokens can submit concepts and read, pause, resume, cancel or download
runs belonging to their selected accounts. Account login, model preferences,
configuration, archive execution, token management and operator WebSockets remain
operator-only. Run and account lists are filtered to the token's scopes.

Operators can list token metadata with `GET /api/tokens` and permanently revoke a
token with `DELETE /api/tokens/{id}`. Revocation is idempotent and survives daemon
restart. Every request checks revocation; revoked tokens receive `401`. Restoring
a backup also restores its authorization state, so rotate credentials after a
restore. The Accounts screen manages these scoped tokens; the standard concept MCP
operations honor the same scopes. The advanced MCP compatibility interface remains
operator-only.

Browser subprocesses inherit an explicit environment allowlist for process and
display prerequisites. Advanced explicit bridge overrides remain operator input.
The old archive bridge's incomplete provider-login behavior is tracked separately
in [the release gap register](public-release-progress.md).

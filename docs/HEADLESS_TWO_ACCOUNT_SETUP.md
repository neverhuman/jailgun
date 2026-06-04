# Jailgun — headless two-account setup (operator walkthrough)

This guide walks you through standing up a **headless jailgun server** that drives **two authenticated
ChatGPT (Google) accounts**, each as its own persistent browser "server" (separate Chrome process, profile,
and CDP port). You log in **once per account** (manually, assisted by a remote view), and the sessions
**persist** across restarts until Google requires re-authentication.

Example placeholders used throughout (substitute your own account ids and emails):

| Account id | Email | Type | CDP port | Profile |
|---|---|---|---|---|
| `chatgpt-a` | `workspace-user@example.invalid` | Google **Workspace** | `9224` | `~/.jailgun/profiles/chatgpt-a` |
| `chatgpt-b` | `personal-user@example.invalid` | personal Gmail | `9225` | `~/.jailgun/profiles/chatgpt-b` |

> "Two servers" here = the **two browser accounts** behind **one** jailgun HTTP server. Each account is an
> isolated Chrome process with its own profile + CDP port — they never share a profile.

---

## 0. Prerequisites (host, once)

A headless Linux box. Google login needs a **headed** Chrome on a **virtual display** (true `--headless`
breaks Google MFA/trust), plus a way to *see* that display for the one-time login.

```bash
# Chrome + virtual display + a VNC bridge for the one-time manual login
sudo apt-get update
sudo apt-get install -y google-chrome-stable xvfb x11vnc novnc websockify nodejs

# Build jailgun (from the repo root)
cd ~/jailgun
cargo build --release -p jailgun-cli            # produces target/release/jailgun
export JAILGUN_BRIDGE_CMD="node $PWD/apps/chrome-bridge/bin/chrome-bridge.mjs"
```

Bring up a virtual display + a noVNC tunnel you'll use only for the one-time logins:

```bash
Xvfb :99 -screen 0 1280x1024x24 &          # virtual screen
export DISPLAY=:99                          # everything below runs against it
x11vnc -display :99 -localhost -rfbport 5999 -nopw -forever &
websockify --web=/usr/share/novnc 6080 localhost:5999 &
# Now open http://<this-host>:6080/vnc.html (tunnel/port-forward it; keep it private).
```

---

## 1. Register + authenticate both accounts (one CLI command)

`jailgun auth setup` registers each email as an account (deriving a stable id, profile dir, and CDP port),
launches its Chrome under your `DISPLAY=:99`, and walks the Google login. **No passwords are stored; the
verification code is never logged.**

```bash
DISPLAY=:99 ~/jailgun/target/release/jailgun auth setup \
  --email workspace-user@example.invalid \
  --email personal-user@example.invalid \
  --registry      ~/.jailgun/browser-profiles.json \
  --profile-root  ~/.jailgun/profiles \
  --state-root    ~/.jailgun/browser-state \
  --downloads-root ~/.jailgun/downloads \
  --cdp-port-start 9224 \
  --prefer-email-code \
  --status-watch \
  --bridge-cmd node ~/jailgun/apps/chrome-bridge/bin/chrome-bridge.mjs \
  --bridge-env JAILGUN_CHROME_HEADLESS=false
```

For **each** account, in turn:
1. The CLI opens that account's Chrome on display `:99`. **Look at it through noVNC** (`http://host:6080/vnc.html`).
2. Complete the Google login in the browser:
   - The Workspace account may get an SSO/SAML redirect or a "verify it's you" device
     prompt. Finish those steps in the noVNC view; the tool **pauses and waits** for you (it never tries to
     bypass a second factor).
   - Approve the "tap **Yes** on your phone" prompt if shown, or choose **"Get a code via email"**.
3. When the **"enter code"** screen appears, the CLI prompts:
   ```
   Enter email verification code for workspace-user@example.invalid: 123456
   ```
   Paste the 6-digit code from your phone/email and press Enter. (Use `--code-stdin` to pipe codes instead
   of an interactive prompt.)
4. On success the account flips to `ready` and the profile is persisted.

Repeat happens automatically for the second account (its Chrome opens on CDP `9225`).

**Result** (`~/.jailgun/browser-profiles.json`):
```json
{ "version": 1, "accounts": [
  { "id": "chatgpt-a", "email_hint": "workspace-user@example.invalid", "cdp_port": 9224,
    "profile_dir": "~/.jailgun/profiles/chatgpt-a", "status": "ready", "last_verified_at": "2026-06-04T10:35:12Z" },
  { "id": "chatgpt-b", "email_hint": "personal-user@example.invalid",  "cdp_port": 9225,
    "profile_dir": "~/.jailgun/profiles/chatgpt-b", "status": "ready", "last_verified_at": "2026-06-04T10:41:08Z" }
]}
```
> Account ids are derived from the email (shown as `chatgpt-a`/`chatgpt-b` here for readability). Profiles
> are created `0700`; the **persistent profile is the durable secret** — keep its volume private/encrypted.

---

## 2. Start the server

```bash
DISPLAY=:99 ~/jailgun/target/release/jailgun serve \
  --addr 127.0.0.1:8787 \
  --live \
  --ingest-token "sk-jailgun-CHANGE-ME"          # protects the control-plane endpoints
```
All control-plane calls below send `-H "x-jailgun-token: sk-jailgun-CHANGE-ME"`.

---

## 3. Verify both accounts are authenticated

```bash
export TOKEN=sk-jailgun-CHANGE-ME ; export S=http://127.0.0.1:8787
curl -s "$S/api/browser/accounts" -H "x-jailgun-token: $TOKEN" | jq '.accounts[] | {id,email_hint,status}'
# { "id": "chatgpt-a", "email_hint": "workspace-user@example.invalid", "status": "ready" }
# { "id": "chatgpt-b", "email_hint": "personal-user@example.invalid",  "status": "ready" }
```

MCP discovery and status are available on the same server:

```bash
curl -s -XPOST "$S/mcp" -H "x-jailgun-token: $TOKEN" -H 'content-type: application/json' -d '{
  "jsonrpc":"2.0","id":"init","method":"initialize","params":{}
}'
curl -s -XPOST "$S/mcp" -H "x-jailgun-token: $TOKEN" -H 'content-type: application/json' -d '{
  "jsonrpc":"2.0","id":"auth","method":"tools/call",
  "params":{"name":"jailgun.auth_status","arguments":{}}
}'
```

**Re-authenticating later** (if a session expires) — same flow over HTTP, no restart:
```bash
curl -s -XPOST "$S/api/browser/accounts/chatgpt-a/auth/start" -H "x-jailgun-token: $TOKEN"   # opens login on :99
# finish the login in noVNC, then submit the code:
curl -s -XPOST "$S/api/browser/accounts/chatgpt-a/auth/code"  -H "x-jailgun-token: $TOKEN" \
     -H 'content-type: application/json' -d '{"code":"123456"}'
```

---

## 4. Run a job — directly or via MCP

Direct HTTP:
```bash
printf 'Refactor the parser and add real tests.' > /tmp/prompt.txt
curl -s -XPOST "$S/api/runs" -H "x-jailgun-token: $TOKEN" -H 'content-type: application/json' -d '{
  "version": 1,
  "prompt_ref": "local://prompt/refactor-parser",
  "prompt_file": "/tmp/prompt.txt",
  "tabs": 2,
  "browser": { "account_ids": ["chatgpt-a"] }
}'
# -> 202 { "run_id": "run-…", "summary_url": "/api/runs/run-…/agent-summary" }
curl -s "$S/api/runs/run-…/agent-summary" -H "x-jailgun-token: $TOKEN" | jq .status
```

Via **MCP** (so jmcp/ZYAL call jailgun directly — JSON-RPC over `/mcp`):
```bash
curl -s -XPOST "$S/mcp" -H "x-jailgun-token: $TOKEN" -H 'content-type: application/json' -d '{
  "jsonrpc":"2.0","id":"1","method":"tools/call",
  "params":{"name":"jailgun.run","arguments":{
     "browser":{"account_ids":["chatgpt-b"]},
     "prompt_ref":"jmcp://prompt/42","prompt_file":"/tmp/prompt.txt","tabs":2}}}'
```

Progress/status tracking for JMCP:

```bash
curl -s -XPOST "$S/mcp" -H "x-jailgun-token: $TOKEN" -H 'content-type: application/json' -d '{
  "jsonrpc":"2.0","id":"status","method":"tools/call",
  "params":{"name":"jailgun.run_status","arguments":{"run_id":"run-…"}}
}'
curl -s -XPOST "$S/mcp" -H "x-jailgun-token: $TOKEN" -H 'content-type: application/json' -d '{
  "jsonrpc":"2.0","id":"summary","method":"tools/call",
  "params":{"name":"jailgun.run_summary","arguments":{"run_id":"run-…"}}
}'
```

Canonical run routing is `browser.account_ids`. REST also accepts top-level `account_ids`, and MCP
also accepts top-level `account`, as compatibility aliases at ingress only. Do not pass raw Chrome
profile paths for `jmcp://` runs; register accounts and use account ids.

To register Jailgun from JMCP after the Jailgun server is reachable:

```bash
jmcpctl jailgun setup \
  --url "$S" \
  --token "$TOKEN" \
  --allowed-url 'jmcp://*'
jmcpctl jailgun doctor
jmcpctl jailgun status
```

---

## 5. Persistence & "stop thinking about it"
- Sessions live in the per-account `--user-data-dir` and **survive server restarts** — restart `serve` and a
  run still works without re-login.
- If a session does expire, the account flips to `auth-required`; re-run the §3 re-auth flow once.

---

## Status — implemented contract and operational caveats
**Working now:** `auth setup` (register + login both accounts, code via stdin), the browser-accounts API
(`/api/browser/accounts/*` start/code/status/restart/stop), per-account profile isolation + CDP ports,
canonical `browser.account_ids` routing, the `/mcp` tools
(`jailgun.run`/`auth_status`/`submit_code`/`run_status`/`run_summary`), and persistent profiles.

**Operator caveats:**
1. **Xvfb/noVNC** is operator-provided (this doc's §0); Jailgun verifies the environment but does not auto-spawn it.
2. **Session expiry** marks the account `auth-required`; active work fails cleanly and the operator re-authenticates.
3. **Run submission** remains compatible through `/api/runs`; `/mcp` is primary for discovery, auth status, run status, and run summaries.
4. **CDP/noVNC** should stay local, tunneled, or otherwise authenticated. Do not expose raw CDP publicly.

## Troubleshooting / safety
- **CAPTCHA / passkey / SMS-only / Workspace-SSO** → the flow **pauses** and waits for you to finish that
  step in noVNC; it never bypasses a second factor.
- **Codes are never written to logs.** Passwords are never stored — only the persistent profile.
- Profile-lock on restart → remove `~/.jailgun/profiles/<id>/Singleton{Lock,Cookie,Socket}` before relaunch.

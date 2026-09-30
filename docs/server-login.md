# Linux server login

Run the concept daemon as your own user. Chrome keeps its sandbox enabled.
The managed browser viewer requires `Xvfb` and `x11vnc`. Jailgun checks for these
programs and never installs privileged packages implicitly. On Ubuntu, install
the dependencies explicitly:

```bash
sudo apt-get update
sudo apt-get install xvfb x11vnc xauth x11-utils
jailgun doctor --server-browser
jailgun setup --server-browser --no-open
```

The daemon listens on loopback. From your desktop, forward the dashboard port
printed by setup; for the default port 8787, use:

```bash
ssh -N -L 8787:127.0.0.1:8787 your-user@your-server
```

Replace `your-user@your-server` with your SSH destination. Open
`http://127.0.0.1:8787`, enter the single-use pairing code from setup, and choose
**Accounts → Connect ChatGPT → Open login view**. Use a current Chrome browser. The dashboard requires JavaScript modules and
top-level await support; other dashboard browsers have not been verified.

Complete the website's normal login and verification in the embedded browser.
Passwords and verification codes stay in that browser; Jailgun does not collect
them in forms, logs or database records. Clipboard sharing is disabled. On a
keyboard, choose **Focus login browser**, use Tab within the remote page, and
press Escape to return to dashboard controls. Choose **Use actual size** to
read small controls and drag to pan; click without dragging to interact.
Confirm the detected account and
choose an observed model when Jailgun asks.

The view closes after confirmation, cancellation or the 15-minute login expiry.
Closing the view alone keeps login active and preserves the private browser
profile. **Reconnect** starts another login window without removing completed
results or changing the account's profile. Restart the service with
`--server-browser` to keep using managed displays.

Each account has its own authenticated X display. X11 TCP access is disabled.
Chrome uses a private Playwright pipe; no CDP listener is opened. The Rust
WebSocket proxy starts x11vnc in `-inetd` mode over private socket descriptors,
so there is no VNC network listener. Only a paired operator browser session
with the correct Origin can reach the proxy; operator bearer credentials and
automation tokens cannot open it. One viewer may connect to an account at a
time. The proxy rechecks session and login validity while connected, bounds
message sizes, and closes stalled transports.

X authority files live under the account's private state directory. They are
created with mode 0600, never copied into release assets, and retained as
private runtime state. Daemon shutdown stops its owned viewers and displays.
Linux parent-death signals stop them if the daemon is killed abruptly. A login
view never stops a browser that is serving a concept run.

If the viewer cannot connect, close another open view, run
`jailgun doctor --server-browser`, and reconnect. If pairing expired, rerun
`jailgun setup --server-browser --no-open` to obtain a new code. Do not expose
the dashboard through a public reverse proxy or open VNC/CDP ports.

The automated lane verifies this path with a synthetic provider and sandboxed
Chrome, including keyboard login, cancellation, expiry, account confirmation
and concept generation. Live ChatGPT server acceptance is still pending.

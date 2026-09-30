# Optional automatic startup

`jailgun service unit` writes a native user-service definition and prints its
activation commands. It does not execute those commands. Stop the existing
daemon first; generation refuses an owned runtime and never overwrites an
existing file. Definitions and the runtime log are private files. They contain
paths and launch options, with no credentials.

Use a complete installed bundle and run `jailgun doctor` with the intended
browser mode first. A source checkout needs explicit built `--assets` and
`--node` paths. Managed installations use the stable `PREFIX/bin/jailgun` link,
so the next service start uses the active installed version. Manual archive and
development executables retain their absolute path. The command preserves an
existing daemon port unless `--addr` is given; only loopback is accepted.

## Linux systemd user service

For a server with the [private dashboard login viewer](server-login.md):

```bash
jailgun doctor --server-browser
jailgun service stop
mkdir -p "$HOME/.config/jailgun"
jailgun service unit --server-browser --out "$HOME/.config/jailgun/jailgun.service"
cat "$HOME/.config/jailgun/jailgun.service"
systemctl --user link "$HOME/.config/jailgun/jailgun.service"
systemctl --user daemon-reload
systemctl --user enable --now jailgun.service
jailgun service status
jailgun setup --no-open
```

Use SSH forwarding and the one-use pairing URL as described in the server guide.
The unit starts with the user's service manager. Keeping that manager running
after logout or starting it at boot depends on the host's user-lingering policy;
an administrator may explicitly enable it with `loginctl enable-linger USER`.
Jailgun does not change that host policy. A graphical desktop can omit
`--server-browser` when its user service manager has the correct display
environment. `--headless` requires an already authenticated browser profile and
does not provide first login.

The service retries failures after ten seconds, with at most three starts in
five minutes. Fix the reported prerequisite or runtime problem, then use
`systemctl --user reset-failed jailgun.service` and `systemctl --user start
jailgun.service`. `jailgun service stop` exits successfully and stays stopped;
the enabled service may start again at the next user-manager startup. Disable
it to prevent that:

```bash
systemctl --user disable --now jailgun.service
```

Retain the definition for later use, or remove only the reviewed definition and
its link after disabling it, then run `systemctl --user daemon-reload`. Profiles
and results are separate and remain intact.

## macOS launchd user agent

The agent runs in the logged-in user's graphical session. It is not a system
daemon and does not provide a login window before the operator signs in.

```bash
jailgun doctor
jailgun service stop
mkdir -p "$HOME/Library/LaunchAgents"
jailgun service unit --out "$HOME/Library/LaunchAgents/com.jailgun.plist"
plutil -lint "$HOME/Library/LaunchAgents/com.jailgun.plist"
cat "$HOME/Library/LaunchAgents/com.jailgun.plist"
launchctl enable "gui/$(id -u)/com.jailgun"
launchctl bootstrap "gui/$(id -u)" "$HOME/Library/LaunchAgents/com.jailgun.plist"
jailgun service status
jailgun setup
```

The LaunchAgents location also makes the agent eligible for loading at the next
graphical login. After an explicit `jailgun service stop`, restart the loaded
agent with `launchctl kickstart "gui/$(id -u)/com.jailgun"`. To prevent future
startup and unload the current job:

```bash
launchctl disable "gui/$(id -u)/com.jailgun"
launchctl bootout "gui/$(id -u)/com.jailgun"
```

Remove the reviewed plist only if no longer needed. If it is already unloaded,
`bootout` reports that the job was not found. Failures are retried with a
ten-second throttle; successful operator shutdown is not retried. Repeated
failures require inspecting the private `daemon.log` and running `doctor`.

## Changes and validation

Finish or pause active runs, stop and disable/unload the native job, then generate
a new definition at a new output path when changing runtime or browser options.
Do not run two differently configured jobs against the same runtime. The daemon
ownership lock rejects that configuration. Credentials remain in the private
runtime; do not add them to service definitions or command arguments. Startup
does not enable advanced deployment.

The native package lane validates the generated definition, starts an isolated
installed daemon, forces a failure through its exact native job, verifies
restart with the same credential, and checks that operator shutdown remains
stopped before unloading the job. Linux is exercised locally; macOS execution
is required in the platform matrix before claiming verified macOS support.

Native behavior follows [systemd.service](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html)
and Apple's [launchd guidance](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html).

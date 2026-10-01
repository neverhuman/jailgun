# Installation and upgrades

v0.2.0 is under development. Native Linux synthetic checks have passed; public
packages, complete clean-environment acceptance, macOS acceptance and live ChatGPT checks
are still pending. Do not treat a development archive as a verified public release.

An isolated Ubuntu installation check covers setup, dashboard serving, daemon
reuse and backup/restore without a checkout or developer toolchain. Browser
workflow acceptance in that clean environment remains pending.

## Clone and build

Install Git, curl, tar, a SHA-256 utility, Rust through
[rustup](https://rustup.rs/), and a C compiler. The checkout pins Rust in
`rust-toolchain.toml`. On macOS, install Xcode Command Line Tools with
`xcode-select --install`. Then:

```bash
git clone https://github.com/neverhuman/jailgun.git
cd jailgun
bash scripts/bootstrap.sh
export PATH="$HOME/.local/bin:$PATH"
jailgun doctor
jailgun setup
```

These commands require the implementation to be merged into public `main`.
Until then, check out the development branch containing the bootstrap script.
Use `--prefix "/another/user-owned directory"` to select an installation prefix;
the installer prints the exact PATH command. Add that command to your shell's
startup file if needed.

Installation paths may contain spaces. Control characters and backslashes are
rejected because Node cannot load runtime modules from a backslash path. A
symbolic-link alias does not remove that restriction on the resolved path.

Bootstrap downloads checksum-pinned Node 24 and Syft, installs locked
`cargo-auditable`, installs locked npm dependencies, compiles the binaries,
bridges and dashboard, generates notices and the actual bundle SBOM, and installs
the verified archive. Build tools and downloads stay under this checkout's
`target/`; installed operation needs no Rust, npm or source checkout. It reuses
the checkout's Cargo build directory. `--tools-only` installs just the pinned
build tools and prints their PATH. `--allow-dirty` explicitly marks a local build
as development evidence.

On Ubuntu 24.04 or 26.04, explicitly request installation of build and server
display dependencies with:

```bash
bash scripts/bootstrap.sh --install-dependencies
```

That option invokes `sudo apt-get` for build tools, curl, certificate roots,
Xvfb, x11vnc and X utilities. Without it, bootstrap does not invoke a privileged
package manager. Rust and [Google Chrome](https://www.google.com/chrome/) remain
operator-installed prerequisites. Chrome's sandbox stays enabled.

## Release archive

Once the version is published, download `install.sh`, `SHA256SUMS` and your
platform archive from the same
[official release](https://github.com/neverhuman/jailgun/releases). Review the
installer and verify its checksum before executing it. For example:

```bash
sha256sum --check SHA256SUMS --ignore-missing
bash install.sh --archive jailgun-0.2.0-linux-x64.tar.gz \
  --checksums SHA256SUMS
```

On macOS, select the ARM or Intel archive and use `shasum -a 256` to compare the
downloaded installer/archive hashes with `SHA256SUMS`. Only run a file whose
listed hash matches. Alternatively, the verified installer can download a
specific release with `bash install.sh --version 0.2.0`.

The installer checks the archive checksum before extraction, rejects links and
unsafe archive paths, verifies the full manifest, and switches the active
application version atomically. Linux x86-64 and macOS ARM/Intel are the intended
bundle targets. Other platforms remain unverified. `bundle.json` records the
actual signing status; do not infer Apple notarization from a checksum.

## First login and server access

On a desktop, `jailgun setup` opens the dashboard. Connect an account, complete
ChatGPT's normal login in its dedicated browser window, verify the observed
identity and choose a model. Jailgun preserves that private profile for reuse.

On a Linux server, run:

```bash
jailgun doctor --server-browser
jailgun setup --server-browser --no-open
```

From your workstation, forward the loopback dashboard port:

```bash
ssh -N -L 8787:127.0.0.1:8787 user@your-server
```

Open the pairing URL printed by setup in the workstation browser. It expires
after five minutes and works once; rerun setup for a fresh link. The dashboard's
operator session provides the private browser view. CDP and VNC are not exposed
as remote public services. Keep the SSH forward open while using the dashboard.

## Upgrade and recovery

Install a new verified archive with the same prefix. Existing versions and all
profiles/results under `~/.jailgun` (or your explicit runtime root) are retained.
An already running daemon keeps its original executable and assets until
restarted. Finish or pause active work and run `jailgun service stop` before starting the
new version. Optional [systemd-user and launchd startup](startup.md) uses a native
definition generated for review and explicit activation.

Before upgrading, run `jailgun service stop --runtime "$HOME/.jailgun"`, then
`jailgun data backup --runtime
"$HOME/.jailgun" --out "$HOME/jailgun-backups/before-upgrade"` with the currently
installed binary. Create the private backup parent first; the destination must
be new. Restoring older application
files alone does not roll back a database. See [database operations](../db/README.md)
and the [runtime layout](runtime-layout.md). Installer staging directories are
retained under your cache for diagnosis; they are separate from account data.

## Remove application files

Finish or pause active work. Disable and unload any optional native startup job
using the [startup guide](startup.md), then stop each runtime using this
installation:

```bash
jailgun service stop --runtime "$HOME/.jailgun"
jailgun uninstall
```

Uninstall removes registered application versions and the two managed executable
links. It preserves accounts, profiles, credentials, results, backups, unrelated
programs in the installation prefix, and service definitions. It does not signal
processes from saved PIDs. Every running Jailgun or jailhard operation holds an
application-use lock; another runtime using the same installation also blocks
removal. Older bundles without that lock protocol require manual inspection and
are refused by automatic removal.

Hashes, sizes, executable permissions and links must match the installation
inventory. Modified or unregistered files stop removal and remain available for
inspection. Interrupted removal retains a private `.uninstall.json` journal.
If its executable was already removed, run `jailgun uninstall --prefix PREFIX`
from a fresh verified archive to resume. Do not reinstall over that journal.
An installer lock left by a terminated process also requires inspection before
retrying; Jailgun never assumes a saved PID proves a lock is stale.

Manual archive installations have no managed inventory. Stop their processes
and remove only the extracted application directory. Runtime data is separate;
application removal never implies permission to purge it.

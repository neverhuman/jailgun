# Runtime assets and data

The application data root defaults to `~/.jailgun`. `--runtime` or
`JAILGUN_RUNTIME` selects another private local directory. Profiles, SQLite state,
operator credentials, daemon logs and captured results belong there, outside the
application installation. Upgrading application files must preserve that data.

An installation contains `bin/jailgun`, `bin/jailhard` and `lib/jailgun/`.
Both executables resolve runtime assets relative to their actual executable
location, including when invoked through a symlink or from another working
directory. `JAILGUN_ASSETS` (or `--assets` for concept commands) explicitly selects
a development asset directory. The installation contains:

- `lib/jailgun/node/bin/node` and the complete upstream Node LICENSE.
- Compiled archive and concept bridges in `lib/jailgun/apps/chrome-bridge/bin/`.
- The bridge's production dependencies in `lib/jailgun/node_modules/`.
- Built dashboard and viewer assets in `lib/jailgun/apps/dashboard/dist/`.
- The shipped advanced configuration in `lib/jailgun/config/jailgun.example.toml`.
- Dependency license files, noVNC corresponding source and SBOM under
  `share/jailgun/`.

Advanced commands first honor a configuration file at the requested path. Only
the missing default `config/jailgun.example.toml` resolves to the installed
default. A missing custom configuration fails; it never reads from the machine
where Jailgun was compiled. Explicit bridge arguments remain supported.
`JAILGUN_BRIDGE_CMD` also accepts a JSON array of arguments for paths containing
spaces; its older whitespace-separated form remains compatible.

Local `jailgun mcp` uses the same daemon discovery and safe startup as other local
commands. It uses an explicit private token file or `JAILGUN_TOKEN` when supplied,
otherwise the local operator credential. Give automation clients a separately
revocable scoped token to limit their access. `--no-start` disables startup, and
an explicit `--url` requires an already running daemon or SSH forward. MCP
protocol messages use stdout; diagnostic output uses stderr.

## Distribution builder under verification

`node scripts/build-bundle.mjs` builds the native Linux x86-64, macOS ARM or macOS
Intel bundle on its corresponding host. It requires the pinned Node and Rust
toolchains, curl, tar, cargo-auditable and Syft. Distribution tool versions live
in `ops/distribution/tools.json`. It installs locked npm dependencies, typechecks,
compiles both bridges and the dashboard, builds Rust with source-path remapping,
and records source and file hashes. The pinned Node archive is checksum-verified
before extraction. Release candidates must have a clean source tree; a local
`--allow-dirty` build explicitly records its development status.

Outputs stay in a new directory under the checkout's `target/distribution/`.
`--out target/distribution/<name>` chooses that directory and refuses to reuse an
existing one. The bundle records source identity and signing status. Linux
binaries are unsigned. On macOS, the builder inspects each executable with
`codesign`, distinguishes unsigned, ad-hoc and other signatures, and verifies
existing signatures. This includes the upstream Node binary; it does not imply
Jailgun has a Developer ID signature. Notarization is recorded as not performed.
The checksum file covers the complete archive. A successful build is not package
acceptance, live-provider acceptance, signing, notarization or publication.
`bash ops/ci/package.sh` builds and exercises installation, repeated installation,
checksum failure, startup, pairing, private APIs and daemon reuse. It runs from
a directory containing spaces with a system-only PATH. This local proof leaves
the build checkout present; it does not establish clean-environment acceptance.
The Linux lane additionally checks installation in a pinned Ubuntu container
without the checkout or developer toolchain. It covers setup and persistence
operations with an actionable missing-Chrome check; complete browser workflows
in that clean environment remain outstanding. See [testing](testing.md).
`JAILGUN_PACKAGE_ALLOW_DIRTY=1` permits a clearly marked local development proof;
GitHub Actions rejects that option.

## Install an archive

Download the version's archive and `SHA256SUMS` from the same official release.
Run the versioned installer from that release:

```bash
bash install.sh --version 0.2.0
```

For a local candidate or manually downloaded archive:

```bash
bash scripts/install.sh --archive /path/to/jailgun-0.2.0-linux-x64.tar.gz \
  --checksums /path/to/SHA256SUMS --prefix "$HOME/.local"
```

The installer verifies the archive checksum before extracting or executing it,
rejects links and unsafe paths, and verifies every installed file against the
bundle manifest. It uses only Bash, curl, tar, awk and a SHA-256 utility from the
host; Node comes from the verified archive. Release download instructions become
usable only after that version is published. v0.2.0 is currently unreleased.

The prefix contains stable `bin/jailgun` and `bin/jailhard` symlinks and a managed
`lib/jailgun/releases/` directory. Installation copies into a fresh directory,
verifies it, then atomically switches `lib/jailgun/current`. It refuses unrelated
executables and directories. An installation lock prevents concurrent upgrades;
an interrupted installer never silently steals a stale lock. Inspect the lock's
owner and stop any surviving installer before removing a stale lock.

Upgrades retain previous application versions and never relocate runtime data.
An already running daemon retains its executable and assets until restarted;
restart it deliberately after saving work. Reinstalling the same verified
archive is idempotent. Installer downloads and interrupted copies are retained
for diagnosis. Database rollback requires restoring a compatible backup; do not
point an older binary at a newer database schema.

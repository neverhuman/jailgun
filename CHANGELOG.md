# Changelog

## Unreleased

Release preparation; v0.2.0 is not yet complete or accepted. See
[the gap register](docs/public-release-progress.md).

- Add a lightweight browser-free worker daemon with operator-only MCP tools for
  isolated tabs, idempotent Codex jobs, Astra/Sol selection, reasoning effort,
  timeouts, cancellation, error reporting and verified tar.gz object transfer.
- Add isolated multi-account worker profiles, explicit tab-to-account routing,
  login-readiness counts and automatic relogin marking after authentication
  failures without moving credentials through MCP.
- Exercise two distinct persistent synthetic provider identities through the
  production browser supervisor, including independent expiry/cancellation,
  a complete workflow on the unaffected account and browser restart reuse.
- Managed uninstall verifies registered files and links, refuses active use or
  changed inventories, retains an interruption journal, and preserves runtime
  data. Installed Jailgun and jailhard processes share a stable use lock across
  upgrades; bundles advertise the locking protocol explicitly.
- Add five-to-ten-perspective exploration, weighted comparison, synthesis,
  critique and final revision through the dashboard, CLI and standard MCP.
- Add the durable SQLite concept service, account-wide pacing and budgets,
  verified artifacts, retained partial results and conservative restart recovery.
- Add offline backup/restore commands with schema checks, private directories,
  exclusive ownership, artifact verification and completed-result retention.
- Add operator-only daemon status and graceful stop commands, with instance and
  runtime verification, ownership-release checks and idempotent local shutdown.
- Generate private systemd-user and launchd definitions with stable installed
  executable paths, explicit activation and graceful stop behavior; verify native
  service lifecycle in the package lane.
- Add operator account onboarding, model confirmation, reconnect and the private
  Linux dashboard login viewer. Reconnect refreshes observed model availability
  while preserving the account allocation and completed results.
- Serialize account readiness checks with reconnect, confirmation and cancellation;
  prevent an in-flight check from overwriting a newer login action.
- Issue dashboard pairing links only on an explicit operator request; daemon
  startup no longer writes pairing secrets to process logs.
- Keep local CLI and MCP credentials out of environment-configured HTTP proxies;
  both transports explicitly connect directly to the loopback daemon.
- Add executable-relative runtime assets, pinned Node bundles, dependency notices,
  actual-bundle SBOMs, checksum-verified versioned installation and clone/build
  bootstrap. Clean-environment and macOS package acceptance remain pending.
- Add isolated Ubuntu installation checks without the source tree or developer
  toolchain, covering setup, private dashboard access, daemon reuse and offline
  backup/restore. Clean-environment browser workflows remain pending.
- Add an isolated ChatGPT text adapter and interactive browser fixtures covering
  model selection, persistent login, turn ownership, and honest response capture.
- Isolate browser reservation releases, preserve account setup bindings, and
  serialize registry allocation across processes.
- Share atomic profile ownership between migration and account registration;
  reject linked/foreign markers and keep archive scheduling blocked when an
  ownership marker is dangling or unreadable.
- Create private runtime paths and credentials; hold a daemon ownership lock.
- Authenticate private HTTP/MCP/WebSocket surfaces, add dashboard pairing, and
  remove query-string WebSocket tokens. See [upgrade notes](docs/authentication-upgrade.md).
- Require an authenticated provider identity for login and preserve manual login
  sessions while the operator acts. Live-provider acceptance remains outstanding.
- Preserve diagnostic response text without reporting a failed generation as a
  successful artifact download; preserve unverified Chrome profile locks.
- Replace silent dashboard demo fallback and synthetic UX reports with explicit
  demo mode and real sandboxed browser/axe checks.
- Pin Node 24.21.0 and update locked dependencies to resolve observed advisories.
- Share pinned validation tools across CI and add Ubuntu 24.04/26.04 and macOS 15
  ARM/Intel package jobs; hosted execution remains pending.
- Add actual five/ten-candidate browser workflows through dashboard, CLI, and
  official SDK MCP clients against the synthetic provider. Live ChatGPT remains
  unverified and is required before release publication.
- Rewrite onboarding documentation around the concept journey, add a genuine
  synthetic dashboard screenshot and check local links, JSON examples and CLI
  help surfaces. These checks do not establish clean-clone or live acceptance.

- Added local CI parity scripts, release readiness checks, contract drift checks,
  UX QA evidence, and copy-code evidence lanes.
- Added agent-readable architecture, boundary, testing, and release documents.

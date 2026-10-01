# Jailgun Architecture

Jailgun is split into a Rust control plane and two TypeScript surfaces.

## Runtime Shape

- `crates/jailgun-core` owns durable data models: configuration, event
  contracts, run snapshots, prompt policy, receipt hashing, tar validation, and
  repository string scanning.
- `crates/jailgun-workflow` owns concept stages, their immutable snapshots,
  transactional scheduling, SQLite storage, and verified response artifacts.
  Dashboard, CLI, HTTP and MCP call this service. Their five/ten-candidate
  synthetic workflows are tested; live-provider acceptance is still required.
- `crates/jailgun-deploy` owns remote cleanup and deploy orchestration through
  `RemoteUploadBackend`, `RemoteJobBackend`, `CiTracker`, and
  `JsonReceiptWriter`. Production SSH/SCP code stays behind these traits.
- `crates/jailgun-orchestrator` owns the browser bridge process, bounded bridge
  readers, run coordination, and deploy queue integration.
- `crates/jailgun-server` exposes authenticated HTTP, Streamable HTTP MCP and
  dashboard WebSockets. It checks operator or account-scoped automation
  authorization and delegates scheduling and durable transitions to Rust services.
- `jailgun worker` is a browser-backed server mode in the same Rust boundary. It
  exposes only operator-authorized worker MCP tools, owns isolated tab workspaces
  and verified tar objects, and executes through the supervised ChatGPT browser
  bridge. Each dashboard-registered account owns a private Chrome profile; tabs
  bind one account and never switch sessions in place.
- `crates/jailgun-cli` owns local commands. `jailgun mcp` is a stdio gateway to
  the same authenticated daemon, with protocol stdout separate from diagnostics.
- `apps/browser-adapter` owns browser DOM interaction helpers only. It may
  locate controls, upload archives, and submit prompts, but it must not own
  policy, deploy safety, or durable receipts.
- `apps/dashboard` owns rendered monitoring UX. It reads Rust API contracts and
  fixture mode data but must not silently reinterpret backend failures.

## Concept Data Flow

1. The operator confirms an observed account identity and model.
2. Rust commits the concept request, immutable configuration and execution budget.
3. The scheduler reserves account capacity and paces exploration conversations.
4. The bridge identifies accepted turns and captures complete or partial Markdown.
5. Verified artifacts and state transitions are committed before events are sent.
6. Comparison, synthesis, critique and revision use the same scheduling and
   persistence boundary. Dashboard, CLI and MCP read the resulting manifest.

Offline backup and restore use the same storage adapter and require exclusive
ownership. Runtime assets resolve from the installed executable; runtime data
remain separate. See [runtime layout](runtime-layout.md) and
[database operations](../db/README.md).

## Advanced Archive Data Flow

1. Configuration is loaded and validated by `JailgunConfig`.
2. Browser automation submits prompt batches and captures source archive
   receipts.
3. Tar and receipt validation happens in Rust before deploy.
4. Deploy staging, remote safety, launcher execution, CI tracking, and receipt
   writing happen through trait-owned Rust backends.
5. Run snapshots and `JailgunEvent` records fan out to the dashboard through
   REST and WebSocket endpoints.

## Concept Runtime

`jailgun serve` takes an installation ownership lock and opens the
SQLite service. One Rust account supervisor owns each persistent browser profile
and bridge process. Browser operations travel over a private process pipe;
Chrome's sandbox remains enabled. Profiles, SQLite, artifacts and credentials
belong beneath the private runtime root (default `~/.jailgun`), with imported
profiles referenced at their existing locations.

Account identity and the chosen model must be observed and confirmed before
submitting. Each run snapshots its request, templates and budgets. Accepted
turns retain their conversation and user-turn IDs for conservative recovery.
Artifact writes and state commits precede completion events. HTTP and both MCP
transports reuse these same transitions and account-wide reservations.

The supported network boundary is loopback with SSH forwarding for remote
operators. On Linux, `--server-browser` gives each account a private Xvfb display
and X authority cookie. The operator-cookie WebSocket proxies a private
x11vnc socket pair; neither X11 nor VNC has a TCP listener. The viewer closes
when the operator session or account login ends. Rust owns process lifetime;
noVNC owns only rendering and input. See [server login](server-login.md).
Advanced archive workflows retain optional CDP attachment over an explicit SSH
forward; they cannot independently schedule concept-owned profiles.

## Lightweight Worker Runtime

Worker mode reuses the same SQLite account registry, supervised Chrome profiles,
and dashboard login path as the concept service. One logical tab maps to one
private workspace, one browser conversation, and one active job. Production uses
the browser bridge; CI injects a mock executor and never contacts ChatGPT.
Input/output tar.gz objects are content-verified and path-checked. The listener
is loopback-only, remote access uses SSH forwarding, and all worker tools require
the operator credential because they can execute trusted-agent work.

## Repair Surface

Failures that cross crate or runtime boundaries should expose an agent-readable
shape with `purpose`, `reason`, `common fixes`, `docs_url`, and `repair_hint`.
The next agent should be able to identify the owner and rerun command from
`docs/testing.md` without searching historical chat.

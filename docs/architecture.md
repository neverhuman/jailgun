# Jailgun Architecture

Jailgun is split into a Rust control plane and two TypeScript surfaces.

## Runtime Shape

- `crates/jailgun-core` owns durable data models: configuration, event
  contracts, run snapshots, prompt policy, receipt hashing, tar validation, and
  repository string scanning.
- `crates/jailgun-deploy` owns remote cleanup and deploy orchestration through
  `RemoteUploadBackend`, `RemoteJobBackend`, `CiTracker`, and
  `JsonReceiptWriter`. Production SSH/SCP code stays behind these traits.
- `crates/jailgun-orchestrator` owns the browser bridge process, bounded bridge
  readers, run coordination, and deploy queue integration.
- `crates/jailgun-server` exposes REST snapshots and WebSocket events from Rust
  types. It must not become the source of deployment or prompt-policy truth.
- `apps/browser-adapter` owns browser DOM interaction helpers only. It may
  locate controls, upload archives, and submit prompts, but it must not own
  policy, deploy safety, or durable receipts.
- `apps/dashboard` owns rendered monitoring UX. It reads Rust API contracts and
  fixture mode data but must not silently reinterpret backend failures.

## Data Flow

1. Configuration is loaded and validated by `JailgunConfig`.
2. Browser automation submits prompt batches and captures source archive
   receipts.
3. Tar and receipt validation happens in Rust before deploy.
4. Deploy staging, remote safety, launcher execution, CI tracking, and receipt
   writing happen through trait-owned Rust backends.
5. Run snapshots and `JailgunEvent` records fan out to the dashboard through
   REST and WebSocket endpoints.

## Remote Chrome Transport

Remote browser access stays CDP-only. The remote machine keeps Chrome bound to
`127.0.0.1` and keeps the browser profile and state local to that machine. The
Mac runs an SSH local forward to `127.0.0.1:9224` and a `launchd` wrapper so
the tunnel comes back after login or disconnect. `chrome-bridge` still attaches
through `JAILGUN_CDP_URL`, so the Mac sees only the forwarded CDP endpoint and
never needs the full browser UI.

## Repair Surface

Failures that cross crate or runtime boundaries should expose an agent-readable
shape with `purpose`, `reason`, `common fixes`, `docs_url`, and `repair_hint`.
The next agent should be able to identify the owner and rerun command from
`docs/testing.md` without searching historical chat.

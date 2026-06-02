# Jailgun Architecture

Jailgun is split into a Rust control plane and two TypeScript surfaces.

## Runtime Shape

- `crates/jailgun-core` owns durable data models: configuration, event
  contracts, run snapshots, prompt policy, receipt hashing, tar validation, and
  repository string scanning.
- `crates/jailgun-deploy` owns remote cleanup and deploy orchestration through
  `RemoteUploadBackend`, `RemoteJobBackend`, `CiTracker`, and
  `JsonReceiptWriter`. Production SSH/SCP code stays behind these traits.
- `crates/jailgun-notify` owns the local JMCP outbox writer and the
  structured envelope payloads for notifications and batch launch
  requests.
- `crates/jailgun-orchestrator` owns the browser bridge process, bounded bridge
  readers, run coordination, browser-profile assignment, and deploy queue
  integration.
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
   receipts. When a profile pool is configured, the orchestrator assigns each
   tab to a concrete profile directory and the Chrome bridge launches one
   managed browser per profile on sequential local CDP ports.
3. Tar and receipt validation happens in Rust before deploy.
4. Deploy staging, remote safety, launcher execution, CI tracking, and receipt
   writing happen through trait-owned Rust backends.
5. JMCP batch-request envelopes are written locally, then the JMCP bridge
   creates approval challenges and fans out child work orders after approval.
6. Run snapshots and `JailgunEvent` records fan out to the dashboard through
   REST and WebSocket endpoints, where the UI derives batch quality from
   child outcomes and evidence kinds. Tab snapshots preserve
   `browser_profile`, `browser_profile_dir`, `browser_slot`, and `cdp_url` so
   operators can trace each child run back to the logged-in Google profile.

## Browser Profile State

The Chrome bridge is the only owner of managed Chrome process state. It writes
an aggregate `managed-browsers.json` under `JAILGUN_CHROME_STATE_DIR` and a
per-profile `managed-browser.json` under `profiles/<profile>/`. Profile
directories, state directories, downloads, receipts, and browser logs are local
runtime state and must stay ignored by Git.

## Repair Surface

Failures that cross crate or runtime boundaries should expose an agent-readable
shape with `purpose`, `reason`, `common fixes`, `docs_url`, and `repair_hint`.
The next agent should be able to identify the owner and rerun command from
`docs/testing.md` without searching historical chat.

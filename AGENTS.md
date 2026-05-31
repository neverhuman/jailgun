# Jailgun Agent Instructions

Read `agent/JANKURAI_STANDARD.md` first. For full policy detail, read
`docs/agent-native-standard.md`.

Use RTK-prefixed commands when RTK is available in the local agent environment.
Keep secrets, browser profiles, downloaded archives, logs, receipts, real
prompts, and local override config out of committed files.

Do not edit outside the requested ownership scope. Run the mapped proof lane
from `agent/test-map.json` before final handoff.

Durable orientation lives in `docs/architecture.md`, `docs/boundaries.md`,
`docs/testing.md`, and `docs/release.md`. Contracts-specific instructions live
in `contracts/AGENTS.md`; ops-specific instructions live in `ops/AGENTS.md`.

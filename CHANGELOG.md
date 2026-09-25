# Changelog

## Unreleased

## [0.2.0] - 2026-09-25

### Added

- Headless auth control plane for persistent browser accounts. The server can
  start, stop, and restart an account, begin an operator-driven auth session,
  and accept a verification code from the CLI or
  `POST /api/browser/accounts/{id}/auth/code` (and the `/api/browsers/...`
  aliases). Passwords are not stored.
- MCP endpoint `/mcp` with `jailgun.run`, `jailgun.auth_status`,
  `jailgun.submit_code`, `jailgun.run_status`, and `jailgun.run_summary`.
- HTTP run ingestion on `POST /api/runs`, including canonical
  `browser.account_ids` routing (REST `account_ids` and MCP `account` aliases
  are normalized onto that field), run snapshots, agent summaries, receipts,
  and `POST /api/events`.
- `jailhard --include-manifest` to build a source archive from an exact file
  list while still applying the security denylist and size cap.
- Passive mouse-activity jitter while a monitored tab is waiting.
- Loopback SSH helpers for a remote Chrome DevTools endpoint
  (`scripts/chrome-cdp-tunnel.sh`, `scripts/chrome-cdp-launchd.sh`).
- Local CI parity scripts, release readiness checks, contract drift checks,
  UX QA evidence, and copy-code evidence lanes.
- Agent-readable architecture, boundary, testing, and release documents.

### Changed

- Split deploy shell backends, browser-adapter DOM contracts, orchestrator
  run/agent/bridge modules, the server browser/MCP/run surface, and the CLI
  command modules out of the previous shape-heavy files.
- Ignore `managed-browsers.json` so browser-profile runtime state stays out of
  Git.

### Fixed

- Formatted the include-manifest excluded-directory test so `cargo fmt --check`
  passes, and bumped Vite to 6.4.3 and Vitest to 4.1.11 so the locked PostCSS,
  nanoid, browserslist, and baseline-browser-mapping packages clear the
  high-severity npm audit range.
- Artifact and tar recovery: generic ChatGPT artifact downloads, current-page
  and tar-indexed browser downloads, malformed artifact sandbox responses,
  artifact-safe tab prompt prefixes, tar download false positives, fail-fast
  when a browser artifact is missing, and recovery of stale managed Chrome CDP
  listeners.

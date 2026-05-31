# Jailgun Jankurai Bootstrap

Standard version: `0.9.0`

Jailgun follows a Rust-core, TypeScript/React surface layout:

- Rust owns configuration, prompt policy, tar validation, receipts, deploy
  safety, run/event models, and the HTTP/WebSocket API.
- TypeScript owns browser DOM adapters and the dashboard only.
- Shell, SSH, SCP, browser profiles, downloaded archives, logs, receipts, local
  config, and prompts with real task content are runtime state and must remain
  uncommitted.

## Hard Blocks

- No committed secrets, auth material, prompt transcripts, browser profiles, or
  private machine paths.
- No project-specific defaults for remote host, remote directory, repository,
  account email, or real prompt text.
- No GitHub write/tool prompt is allowed by default. Information-only tool
  prompts require explicit policy.
- Remote deploy reset must stop if the checkout is dirty, `origin/main` is
  missing, preservation ref creation fails, or receipt writing fails.
- Shell access must go through a Rust trait boundary so CI can test with fake
  remotes.
- UI changes need component tests and a rendered local check when the app is
  materially changed.

## Ownership

Use `agent/owner-map.json` for path ownership and `agent/test-map.json` for
proof commands. Keep generated artifacts listed in `agent/generated-zones.toml`
out of hand edits.

## Validation

Fast local lane:

```bash
ops/ci/rust.sh
ops/ci/node.sh
ops/ci/scan.sh
```

Full advisory lane:

```bash
ops/ci/jankurai.sh
```


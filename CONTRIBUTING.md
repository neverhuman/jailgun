# Contributing

Read `AGENTS.md`, `agent/JANKURAI_STANDARD.md`, and the ownership and proof maps
before changing code. Rust owns application policy and durable state; browser
operations and the dashboard belong in the Node workspaces.

Use the Rust toolchain in `rust-toolchain.toml` and Node in `.node-version`.
Install locked Node dependencies with `npm ci`. Run `bash scripts/ci-local.sh`
for the integrated local lane. Chromium must support sandboxing; see
[testing](docs/testing.md). No provider login is needed for deterministic tests.

Keep a change focused, explain its behavior, and include regression proof.
Do not commit profiles, prompts from real work, tokens, logs, archives, or test
runtime data. Generated files must be refreshed with their owning tools.

Release work is tracked in [the gap register](docs/public-release-progress.md).
Do not advertise an incomplete or unverified integration as supported.

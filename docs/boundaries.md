# Ownership Boundaries

## Rust Core

`crates/jailgun-core` owns public data contracts and validation. Any change to
events, run snapshots, receipts, tar validation, prompt policy, or configuration
must run:

```bash
bash ops/ci/rust.sh
bash ops/ci/contracts.sh
```

## Deploy

`crates/jailgun-deploy` owns remote SSH/SCP execution, cleanup policy,
launcher scripts, CI tracking, and deploy receipts. Shell access must stay
behind Rust traits so `fake-backends` tests cover the deploy path without real
credentials.

## Orchestrator

`crates/jailgun-orchestrator` owns bridge process IO and run coordination.
Bridge readers must use bounded queues or explicit shutdown paths so child
output cannot grow without backpressure.

## TypeScript

`apps/browser-adapter` owns DOM adapters. Missing controls should raise named
errors rather than returning null to a later caller.

`apps/dashboard` owns monitoring UX. API mode and fixture mode are explicit:
network failures in API mode are surfaced to the UI and tests opt into fixture
data deliberately.

## Contracts

`contracts/json-schema/event.schema.json` and `contracts/fixtures/events/` are
generated contract outputs. Do not hand-edit them after changing event shape;
run:

```bash
bash ops/ci/contracts.sh --write
bash ops/ci/contracts.sh
```

## Runtime State

Secrets, browser profiles, archives, receipts, logs, Telegram tokens, local
config, and real prompt bodies are local runtime state. They remain ignored by
Git and out of docs, tests, fixtures, and generated contracts.

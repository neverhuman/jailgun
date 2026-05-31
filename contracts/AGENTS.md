# Contracts Agent Instructions

This directory owns generated event schema and fixture contracts:

- `contracts/json-schema/event.schema.json`
- `contracts/fixtures/events/`

Do not hand-edit generated contract outputs. Update the generator or Rust event
source, then run:

```bash
bash ops/ci/contracts.sh --write
bash ops/ci/contracts.sh
```

Runtime receipts, real prompts, downloaded archives, browser profiles, logs,
and private paths do not belong in contract fixtures.

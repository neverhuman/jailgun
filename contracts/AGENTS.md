# Contracts Agent Instructions

This directory owns generated event schema and fixture contracts:

- `contracts/json-schema/event.schema.json`
- `contracts/json-schema/workflow.schema.json`
- `contracts/fixtures/events/`
- `contracts/fixtures/workflow/`
- `apps/dashboard/src/generated/`

Event and workflow schema types come from Rust through Schemars. The Node
generator creates shared frontend types using json-schema-to-typescript and
standalone runtime validators using Ajv. Workflow fixtures execute the Rust
storage state machine before normalizing generated identities. Install
the locked Node dependencies before running the contracts lane.

Do not hand-edit generated contract outputs. Update the generator or Rust event
source, then run:

```bash
bash ops/ci/contracts.sh --write
bash ops/ci/contracts.sh
```

Runtime receipts, real prompts, downloaded archives, browser profiles, logs,
and private paths do not belong in contract fixtures.

Ownership:
- Owns: event JSON schema, generated event fixtures, and contract drift proof.
- Forbidden: real prompts, browser state, downloaded archives, operator logs,
  credentials, and unreviewed manual edits to generated fixtures.
- Proof lane: `bash ops/ci/contracts.sh --write` only when source changes require
  regeneration, then `bash ops/ci/contracts.sh` and `bash ops/ci/jankurai.sh`.

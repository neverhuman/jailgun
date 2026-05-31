# DB Agent Instructions

This directory is the only approved home for future durable database schema,
migration notes, and constraint policy.

Current state: no production SQL is committed. Do not add credentials, dumps,
real customer data, local database paths, or ad hoc seed data here.

If SQL is added, route it through `db/migrations/` and `db/constraints/`, update
`agent/boundaries.toml`, and run:

```bash
bash ops/ci/release.sh
bash ops/ci/jankurai.sh
```

# Data Boundary

Jailgun currently has no committed production database schema. If durable data
is added, it belongs under `db/` with the Rust application layer calling adapter
APIs rather than issuing direct database writes from UI, browser, or shell
surfaces.

Database changes require migration review, rollback instructions, lock impact
notes, and backfill notes before release. Durable invariants should use foreign
key, check constraint, and row level security rules where those constraints
express the product policy more reliably than application-only checks.

The local DB proof lane is `bash ops/ci/db.sh`. It verifies the boundary
manifest, migration directory, constraint directory, and absence of committed
runtime database artifacts before release.

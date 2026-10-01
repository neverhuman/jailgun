# Durable invariants

Migration 0001 uses STRICT tables, foreign keys, uniqueness constraints, checks,
and triggers. Account IDs, identities, profile paths, and ports cannot collide.
Runs reference an account; attempts reference their run and task; reservations
reference the same run and attempt. Capacity is enforced transactionally across
all workflow runs, capped at ten while retaining the operator's configured limit.
Reservation ownership is a UUID, never a process ID and timestamp pair.

Migration 0002 stores the accepted user turn. Reattachment requires that turn,
the conversation reference and observed model. Resume after restart requires
every retained reservation to be reconciled; an uncertain submission prevents
automatic resume. A prior operator pause remains paused after reattachment.

Run requests, snapshots, idempotency keys, deadlines, and submission limits are
immutable. Reusing a key with different content fails. Submission counts cannot
exceed their persisted hard limit. Terminal runs cannot resume; attempt state
transitions are checked. Cancellation does not free active capacity until the
supervisor acknowledges that the owned conversation has stopped.

Application services check stage prerequisites, complete candidate summaries,
criterion coverage, response ownership, model selection, elapsed deadlines,
context ceilings, and artifact integrity. They preserve original adapter errors.
Partial text cannot advance a stage. Explicit subset continuation requires at
least three complete candidates and no active attempt. Automatic retries are
limited to one per task; explicit retry actions retain the same overall budget.

SQLite has no database-level operator/automation roles in this installation.
Authorization belongs to the Rust service boundary; the private database is
not exposed to clients. This is a single-operator runtime, not tenant isolation.

Proof tests include duplicate imports, conflicting identities, foreign-key
violations, invalid state transitions, immutable configuration, capacity and
pacing across runs, independent cancellation, disk-full rollback, interrupted
artifact writes, corrupt backups, newer schemas, and restart uncertainty.

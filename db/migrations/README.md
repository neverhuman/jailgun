# Migration and rollback

Migration 0001 creates accounts, imports, immutable run configurations, stage
tasks, attempts, reservations, registered artifacts, and sequenced events. It
applies in one transaction to an empty database. Reopening verifies the recorded
migration SHA-256. An unknown version or existing unversioned tables stops startup.

`Store::import_registry` imports version-1 account metadata transactionally,
preserving IDs, profile paths, CDP ports, identity bindings, and operator capacity.
It references profiles in place. It never copies, deletes, or moves profiles.
Duplicate content imports do nothing. Conflicting identities abort the entire
import. A private content-addressed copy of each valid source registry is synced
before the import commits; original files remain untouched. Accounts without a
verified provider binding require login again. Completed legacy archive-summary
import is not implemented yet.

Stop both scheduling and browser use before activating migration. The old JSON
scheduler does not yet consult the new database, so simultaneous operation is
unsupported. The concept daemon now owns its browser supervisor; the advanced scheduler must remain separate.

`Store::backup` runs on the writer queue and uses SQLite's online backup API,
then copies and verifies registered artifacts while writes remain serialized.
`backup.json` is published last. A backup without that manifest is incomplete.
The destination must be new. This operation temporarily blocks workflow writes;
the maximum delay depends on retained artifact size. It does not contain browser
profiles, installation credentials, or external archive/deploy receipts.

`Store::restore` checks database and artifact integrity and schema compatibility,
then creates a new private runtime. It never overwrites an existing runtime.
An interruption leaves `restore.incomplete`, which prevents startup. A restored
runtime must reconcile conversations before scheduling. Browser profiles remain
at their original paths and need a separate backup while Chrome is stopped.

Rollback means stopping the daemon and restoring a verified backup with the
binary that produced its schema, keeping the current runtime intact. An older
binary must not open a newer schema. For the initial JSON-to-SQLite transition,
keep the original JSON files, profiles, and matching old binary; do not run the
old scheduler until new browser ownership has been released and inspected.
There is no automatic down migration or destructive schema rollback.

Proof: `bash ops/ci/db.sh`, `bash ops/ci/rust.sh`, `bash ops/ci/release.sh`, and
`bash ops/ci/jankurai.sh`. Offline backup and restore are exposed through `jailgun data`; profile custody activation remains pending.


Migration 0005 adds immutable request-only identities. Its Rust backfill verifies
historical request hashes against each original model snapshot, then hashes the
serialized request alone. Backfill and schema changes commit together; conflicts
roll back the migration. Original hashes and snapshots remain unchanged.
Normalization materializes typed defaults and field order only: concept whitespace,
Unicode, criterion order and numeric weights are preserved. Matching retries are
looked up before account model resolution and return the original configuration.
The HTTP/MCP authorization gate still checks the requested account first.

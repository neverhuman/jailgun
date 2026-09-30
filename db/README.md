# Durable workflow storage

`crates/jailgun-workflow` owns the SQLite adapter and application operations.
Schema truth is the ordered SQL files in `migrations/`; invariants and proof routing are
described in `constraints/README.md`. Browser and dashboard code never issue SQL.

The adapter bundles SQLite through pinned `rusqlite` 0.40.2 and verifies the
linked version is at least 3.51.3. CI prints the actual linked version. This
includes SQLite's WAL reset correction; WAL requires a local filesystem, not an
NFS/SMB home directory. See the [SQLite WAL documentation](https://www.sqlite.org/wal.html).

One dedicated writer thread owns a connection, with a 64-operation bounded
queue, foreign keys, WAL, FULL synchronization, an exclusive process lock, and
transactional migrations. A newer schema, changed migration checksum, corrupt
database, or second writer fails closed. Public operations return only after
commit; event consumers replay committed sequence numbers and may broadcast
only after that return.

Migration lock waits use SQLite's five-second busy timeout. A progress handler
interrupts migration VM execution after thirty seconds and rolls back the
transaction. This budget does not interrupt a blocked operating-system file
write. The handler is removed before normal workflow operations begin.

Runtime directories and files are created as 0700/0600. Markdown and JSON
artifacts are atomically published under generated UUIDs, synced, and hashed
before completion is committed. Reads require a registered ID, confinement to
the artifact root, and an intact hash. An interrupted write may leave an
unregistered private file; it cannot turn a task into a completed result.

Schema 4 adds digest-only automation credentials and explicit account scopes.
Revocation is permanent and idempotent; token plaintext never enters SQLite.

Schema 3 adds account login sessions, recent identity/model observations and
explicitly selected model preferences. Initial bindings and model confirmation
commit atomically. Restart clears cached observations before verifying Chrome.

Schema 2 persists the exact accepted user turn needed to reattach without
resubmission. Upgrading schema 1 creates a private SQLite before-image before
applying the migration. Attempts without an accepted turn require reconciliation.

The workflow library is exercised through Rust integration tests and the
production Rust supervisor with real Chrome against a synthetic provider.
The authenticated HTTP adapter uses the same services in integration tests.
The concept daemon activates the service with `jailgun serve`; dashboard, CLI,
HTTP and standard MCP operations share these application services.
It migrates the runtime registry in place while holding the archive lease lock,
refuses active archive reservations, and marks profiles as workflow-owned.
Updated archive scheduling and browser startup reject those profiles. Keep older
binaries stopped during migration.

## Backup before upgrading

Finish or pause active runs and stop the owning daemon. Use the currently
installed binary before installing an upgrade. The offline backup command
requires its schema to match the database and never upgrades a database as a
side effect of making a backup:

```bash
mkdir -m 700 -p "$HOME/jailgun-backups"
jailgun service stop --runtime "$HOME/.jailgun"
jailgun data backup --runtime "$HOME/.jailgun" \
  --out "$HOME/jailgun-backups/before-upgrade"
```

The parent directory must exist; the destination must not. An active daemon or
database writer makes the command fail without creating the backup. `--json`
returns a completion record or a structured error on stderr. Backup completion
means SQLite and every registered response artifact have been copied and
verified; `backup.json` records the schema, linked SQLite version and database
hash. A failed backup has no valid completion manifest. Keep it for diagnosis
and retry into a new directory.

The backup contains private concepts, responses, account identity metadata,
profile paths, capacity limits, attempts, execution budgets, events and hashed
automation-token state. Treat it as private account data. It does **not** include
browser profiles, `operator-token`, local configuration, logs, or legacy archive
outputs. Back up those separately if needed, with the daemon and its managed
Chrome processes stopped. Preserve private permissions. Never upload a runtime
backup as public release evidence.

## Restore and rollback

Keep the original runtime intact. Stop its daemon and managed browser processes,
then restore with the application version that created the backup:

```bash
jailgun service stop --runtime "$HOME/.jailgun"
jailgun data restore --backup "$HOME/jailgun-backups/before-upgrade" \
  --out "$HOME/.jailgun-restored"
jailgun setup --runtime "$HOME/.jailgun-restored"
```

Restore accepts only a new destination, verifies the exact copied database and
every artifact hash, checks migration checksums, and holds exclusive daemon and
writer ownership while restoring. A failed restore leaves `restore.incomplete`
and cannot be started as a valid runtime. Preserve that directory for diagnosis;
repair the source backup or choose another verified backup and a new destination.

Account IDs, limits and profile references are preserved. Profiles are neither
copied nor relocated: those original paths must still exist. Never run both
the original and restored installations against the same profiles. If moving
to another host, this command alone does not migrate browser authentication.
Setup creates a new operator credential when it is absent. Automation-token
revocations reflect the backup date; review and revoke restored credentials
before allowing automation to reconnect.

Completed results remain available. Starting the restored daemon reconciles
unfinished attempts; an uncertain submission is not automatically sent again.
Pause state, submission counts and deadlines retain their original values.

For rollback, use the retained matching application bundle and its **pre-upgrade
backup** in a separate runtime. Do not point an older binary at a newer database.
Automatic migration before-images contain SQLite only and are not replacements
for a complete backup with artifacts. Keep the newer runtime and backup until
the restored results and account bindings have been checked.

Run `bash ops/ci/db.sh` to execute migration, constraints, backup/restore,
crash recovery, artifact failure, SQLite disk-full, and workflow tests. The lane
records source identity, tool versions, real exit status, and log hashes in
ignored `target/jankurai/db/`.

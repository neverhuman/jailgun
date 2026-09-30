use crate::{
    artifacts,
    database::{atomic_new, hash, new_private_dir, private_dir, writer_lock, Database},
    Error, Result, Store,
};
use rusqlite::{Connection, OpenFlags};
use std::{fs, path::Path};

impl Store {
    /// Offline restore into a new directory; never overwrites existing accounts or results.
    pub fn restore(backup: &Path, destination: &Path) -> Result<Self> {
        if destination.exists() {
            return Err(Error::action(
                "restore-destination-exists",
                "The restore destination already exists.",
                "Choose a new empty runtime path and keep the existing runtime intact.",
            ));
        }
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(backup.join("backup.json"))?)?;
        let database = fs::read(backup.join("workflows.sqlite3"))?;
        if manifest["schema_version"] != crate::database::SCHEMA_VERSION
            || manifest["database_sha256"].as_str() != Some(hash(&database).as_str())
        {
            return Err(Error::action(
                "backup-integrity",
                "The backup manifest or database hash is invalid.",
                "Use a complete, verified backup.",
            ));
        }
        new_private_dir(destination, "restore-destination-exists")?;
        let destination = destination.canonicalize()?;
        let _owner =
            jailgun_core::installation::Installation::acquire(&destination).map_err(|_| {
                Error::action(
                    "runtime-owned",
                    "Another daemon owns the restore destination.",
                    "Stop the daemon and restore into a new private directory.",
                )
            })?;
        let lock = writer_lock(&destination)?;
        atomic_new(
            &destination.join("restore.incomplete"),
            b"Restoring verified workflow state; do not open until complete.\n",
        )?;
        // Validate the exact hashed bytes that will be used, not a second read of a
        // backup file that could change between hashing and SQLite opening it.
        atomic_new(&destination.join("workflows.sqlite3"), &database)?;
        let source = Connection::open_with_flags(
            destination.join("workflows.sqlite3"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let version: i32 = source.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let application: i32 = source.pragma_query_value(None, "application_id", |r| r.get(0))?;
        let integrity: String = source.pragma_query_value(None, "quick_check", |r| r.get(0))?;
        if version != crate::database::SCHEMA_VERSION
            || application != crate::database::APPLICATION_ID
            || integrity != "ok"
        {
            return Err(Error::action(
                "backup-incompatible",
                "The backup schema or integrity check is incompatible.",
                "Restore with the matching application release.",
            ));
        }
        let artifact_root = backup.join("artifacts").canonicalize()?;
        let registered = {
            let mut stmt = source.prepare("SELECT id FROM runs ORDER BY id")?;
            let ids = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let mut registered = Vec::new();
            for run in ids {
                registered.extend(artifacts::list(&source, &run)?);
            }
            registered
        };
        for artifact in &registered {
            artifacts::read(&artifact_root, artifact)?;
        }
        private_dir(&destination.join("artifacts"))?;
        for artifact in registered {
            atomic_new(
                &destination.join("artifacts").join(&artifact.id),
                &artifacts::read(&artifact_root, &artifact)?,
            )?;
        }
        drop(source);
        // Transfer writer ownership without any unlock/reopen window. Migration
        // checksums are validated before clearing the incomplete marker.
        let database = Database::open_locked(&destination, lock)?;
        fs::remove_file(destination.join("restore.incomplete"))?;
        fs::File::open(&destination)?.sync_all()?;
        Self::from_database(database)
    }
}

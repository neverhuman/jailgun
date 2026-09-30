use crate::{model::Event, Error, Result};
use rusqlite::{Connection, Transaction};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) const SCHEMA: &str = include_str!("../../../db/migrations/0001_workflows.sql");
pub(crate) const SCHEMA_VERSION: i32 = 5;
const MIGRATIONS: [(i32, &str); 5] = [
    (1, SCHEMA),
    (
        2,
        include_str!("../../../db/migrations/0002_accepted_turn.sql"),
    ),
    (
        3,
        include_str!("../../../db/migrations/0003_account_sessions.sql"),
    ),
    (
        4,
        include_str!("../../../db/migrations/0004_automation_tokens.sql"),
    ),
    (
        5,
        include_str!("../../../db/migrations/0005_request_identity.sql"),
    ),
];
pub(crate) const APPLICATION_ID: i32 = 0x4a474e31;

pub(crate) struct Database {
    pub connection: Connection,
    pub root: PathBuf,
    pub artifacts: PathBuf,
    _lock: fs::File,
}

impl Database {
    pub fn open(root: &Path) -> Result<Self> {
        Self::open_inner(root, true)
    }

    pub(crate) fn open_for_backup(root: &Path) -> Result<Self> {
        Self::open_inner(root, false)
    }

    fn open_inner(root: &Path, allow_upgrade: bool) -> Result<Self> {
        if !allow_upgrade && !root.join("workflows.sqlite3").is_file() {
            return Err(Error::action(
                "runtime-not-found",
                "No workflow database exists at this path.",
                "Pass the existing runtime root with --runtime.",
            ));
        }
        private_dir(root)?;
        let root = root.canonicalize()?;
        let lock = writer_lock(&root)?;
        // Check after locking: a concurrent restore must never be opened halfway through.
        if root.join("restore.incomplete").try_exists()? {
            return Err(Error::action(
                "restore-incomplete",
                "This runtime contains an interrupted restore.",
                "Preserve it and restore the verified backup into a new runtime directory.",
            ));
        }
        if !allow_upgrade {
            let existing = Connection::open_with_flags(
                root.join("workflows.sqlite3"),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?;
            let version: i32 =
                existing.pragma_query_value(None, "user_version", |row| row.get(0))?;
            if version != SCHEMA_VERSION {
                return Err(Error::action("backup-version-mismatch", "The backup command requires the installed database schema to match this binary.", "Use the original application version to back up before upgrading; never open a newer schema with an older binary."));
            }
        }
        Self::open_locked(&root, lock)
    }

    pub(crate) fn open_locked(root: &Path, lock: fs::File) -> Result<Self> {
        let path = root.join("workflows.sqlite3");
        private_file(&path, false)?;
        let mut connection = Connection::open(&path)?;
        if rusqlite::version_number() < 3_051_003 {
            return Err(Error::action(
                "sqlite-version",
                "SQLite 3.51.3 or newer is required.",
                "Install a release with patched bundled SQLite.",
            ));
        }
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        let integrity: String = connection.pragma_query_value(None, "quick_check", |r| r.get(0))?;
        if integrity != "ok" {
            return Err(Error::action(
                "database-corrupt",
                "Database integrity verification failed.",
                "Preserve this runtime and restore a verified backup.",
            ));
        }
        migrate(&mut connection, root)?;
        let mode: String = connection.pragma_query_value(None, "journal_mode", |r| r.get(0))?;
        if mode != "wal" {
            connection.pragma_update(None, "journal_mode", "WAL")?;
        }
        let mode: String = connection.pragma_query_value(None, "journal_mode", |r| r.get(0))?;
        if mode != "wal" {
            return Err(Error::action(
                "wal-unavailable",
                "SQLite WAL mode is unavailable.",
                "Place the private runtime on a local filesystem.",
            ));
        }
        connection.pragma_update(None, "synchronous", "FULL")?;
        let integrity: String = connection.pragma_query_value(None, "quick_check", |r| r.get(0))?;
        if integrity != "ok" {
            return Err(Error::action(
                "database-corrupt",
                "Database integrity verification failed.",
                "Preserve this runtime and restore a verified backup.",
            ));
        }
        let artifacts = root.join("artifacts");
        private_dir(&artifacts)?;
        Ok(Self {
            connection,
            root: root.to_path_buf(),
            artifacts,
            _lock: lock,
        })
    }
}

pub(crate) fn writer_lock(root: &Path) -> Result<fs::File> {
    let lock = private_file(&root.join("workflow.lock"), false)?;
    lock.try_lock().map_err(|_| {
        Error::action(
            "database-owned",
            "Another workflow writer owns this runtime.",
            "Use the running daemon or stop it before opening the database.",
        )
    })?;
    Ok(lock)
}

/// Exclusively create the final directory; never adopt an existing directory or link.
/// Its parent must already exist, so this cannot change a parent directory's permissions.
pub(crate) fn new_private_dir(path: &Path, exists_code: &'static str) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            Error::action(
                exists_code,
                "The destination already exists.",
                "Choose a new directory under an existing private parent.",
            )
        } else {
            error.into()
        }
    })
}

pub(crate) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn private_dir(path: &Path) -> Result<()> {
    if path.is_symlink() {
        return Err(Error::action(
            "unsafe-storage-path",
            "A private runtime directory is a symbolic link.",
            "Choose a directory owned by the operator.",
        ));
    }
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub(crate) fn private_file(path: &Path, exclusive: bool) -> Result<fs::File> {
    if path.is_symlink() {
        return Err(Error::action(
            "unsafe-storage-path",
            "A private runtime file is a symbolic link.",
            "Inspect the runtime before continuing.",
        ));
    }
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true);
    if exclusive {
        options.create_new(true);
    } else {
        options.create(true).truncate(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
}

pub(crate) fn atomic_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("missing parent"))?;
    let staging_path = parent.join(format!(".{}.writing", uuid::Uuid::new_v4()));
    let mut file = private_file(&staging_path, true)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    // A link publishes without overwriting an existing artifact. Both paths share a filesystem.
    fs::hard_link(&staging_path, path)?;
    fs::remove_file(&staging_path)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

pub(crate) fn event(
    tx: &Transaction<'_>,
    run_id: &str,
    kind: &str,
    data: serde_json::Value,
    now: i64,
) -> Result<Event> {
    tx.execute(
        "INSERT INTO events(run_id,kind,data_json,created_ms) VALUES(?1,?2,?3,?4)",
        rusqlite::params![run_id, kind, serde_json::to_string(&data)?, now],
    )?;
    Ok(Event {
        sequence: tx.last_insert_rowid(),
        run_id: run_id.into(),
        kind: kind.into(),
        data,
        created_ms: now,
    })
}

pub(crate) fn not_found() -> Error {
    Error::action(
        "not-found",
        "The requested run or artifact does not exist.",
        "List the available runs and use a registered ID.",
    )
}

pub(crate) fn invalid_transition() -> Error {
    Error::action(
        "invalid-transition",
        "The requested action is not valid in the current state.",
        "Refresh the run and choose an available action.",
    )
}

fn migrate(connection: &mut Connection, root: &Path) -> Result<()> {
    let version: i32 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let application: i32 = connection.pragma_query_value(None, "application_id", |r| r.get(0))?;
    if version > SCHEMA_VERSION
        || (version > 0 && application != APPLICATION_ID)
        || (application != 0 && application != APPLICATION_ID)
    {
        return Err(Error::action(
            "schema-incompatible",
            "The database belongs to a different schema or application.",
            "Use the matching binary or restore a compatible backup into a separate runtime.",
        ));
    }
    if version == 0 {
        let tables: i64 = connection.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='table'",
            [],
            |r| r.get(0),
        )?;
        if tables != 0 {
            return Err(Error::action(
                "schema-incompatible",
                "An unversioned database already contains tables.",
                "Preserve it and choose an empty runtime directory.",
            ));
        }
    }
    for (number, sql) in MIGRATIONS {
        if number <= version {
            let checksum: String = connection.query_row(
                "SELECT sha256 FROM schema_migrations WHERE version=?1",
                [number],
                |r| r.get(0),
            )?;
            if checksum != hash(sql.as_bytes()) {
                return Err(Error::action(
                    "migration-drift",
                    "The installed migration differs from the recorded schema.",
                    "Restore the matching release or investigate the database before continuing.",
                ));
            }
        } else {
            if number == version + 1 && version > 0 {
                let backup = root.join(format!(
                    "schema-{version}-before-{number}-{}.sqlite3.backup",
                    uuid::Uuid::new_v4()
                ));
                private_file(&backup, true)?;
                connection.backup(rusqlite::MAIN_DB, &backup, None)?;
                fs::File::open(&backup)?.sync_all()?;
                fs::File::open(root)?.sync_all()?;
            }
            apply_migration(connection, number, sql, Duration::from_secs(30))?;
        }
    }
    Ok(())
}

pub(crate) fn apply_migration(
    connection: &mut Connection,
    number: i32,
    sql: &str,
    budget: Duration,
) -> Result<()> {
    let deadline = std::time::Instant::now() + budget;
    connection.progress_handler(1000, Some(move || std::time::Instant::now() >= deadline))?;
    let result: Result<()> = (|| {
        let transaction = connection.transaction()?;
        transaction.execute_batch(sql)?;
        if number == 5 {
            crate::request_identity::backfill(&transaction)?;
        }
        transaction.execute(
            "INSERT INTO schema_migrations VALUES(?1,?2)",
            rusqlite::params![number, hash(sql.as_bytes())],
        )?;
        transaction.pragma_update(None, "application_id", APPLICATION_ID)?;
        transaction.pragma_update(None, "user_version", number)?;
        transaction.commit()?;
        Ok(())
    })();
    let cleared = connection.progress_handler(0, None::<fn() -> bool>);
    result?;
    cleared?;
    Ok(())
}

use super::*;

pub(super) fn migrate(connection: &mut Connection, root: &Path) -> Result<()> {
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

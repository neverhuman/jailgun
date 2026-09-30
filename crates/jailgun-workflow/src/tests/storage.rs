use super::*;

#[tokio::test]
async fn concurrent_restore_has_one_owner_and_preserves_the_winning_database() {
    let (dir, store) = fixture(7).await;
    let backup = dir.path().join("backup");
    store.backup(backup.clone()).await.unwrap();
    let destination = dir.path().join("restored");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let children: Vec<_> = (0..2)
        .map(|_| {
            let (backup, destination, barrier) =
                (backup.clone(), destination.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                Store::restore(&backup, &destination)
            })
        })
        .collect();
    let mut successes = 0;
    for child in children {
        match child.join().unwrap() {
            Ok(restored) => {
                successes += 1;
                assert_eq!(restored.accounts().await.unwrap()[0].capacity, 7);
                assert_eq!(
                    Store::open(&destination).err().unwrap().code(),
                    "database-owned"
                );
                restored.close().await.unwrap();
            }
            Err(error) => assert_eq!(error.code(), "restore-destination-exists"),
        }
    }
    assert_eq!(successes, 1);
    store.close().await.unwrap();
}

#[tokio::test]
async fn backup_does_not_upgrade_a_mismatched_schema_and_directory_creation_is_exclusive() {
    let (dir, store) = fixture(5).await;
    store
        .call(|db| {
            db.connection.pragma_update(None, "user_version", 3)?;
            Ok(())
        })
        .await
        .unwrap();
    store.close().await.unwrap();
    let root = dir.path().join("runtime");
    assert_eq!(
        Store::open_for_backup(&root).err().unwrap().code(),
        "backup-version-mismatch"
    );
    let source = rusqlite::Connection::open(root.join("workflows.sqlite3")).unwrap();
    assert_eq!(
        source
            .pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
            .unwrap(),
        3
    );
    assert!(!fs::read_dir(&root).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .ends_with("sqlite3.backup")));
    let destination = dir.path().join("existing");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("sentinel"), "keep").unwrap();
    assert_eq!(
        crate::database::new_private_dir(&destination, "already-exists")
            .unwrap_err()
            .code(),
        "already-exists"
    );
    assert_eq!(fs::read(destination.join("sentinel")).unwrap(), b"keep");
    #[cfg(unix)]
    {
        let link = dir.path().join("dangling");
        std::os::unix::fs::symlink(dir.path().join("absent"), &link).unwrap();
        assert_eq!(
            crate::database::new_private_dir(&link, "already-exists")
                .unwrap_err()
                .code(),
            "already-exists"
        );
        assert!(!dir.path().join("absent").exists());
    }
}

#[test]
fn migration_vm_timeout_rolls_back_and_clears_its_handler() {
    let mut connection = rusqlite::Connection::open_in_memory().unwrap();
    connection
        .execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, sha256 TEXT)")
        .unwrap();
    let error = crate::database::apply_migration(&mut connection, 1,
        "CREATE TABLE unfinished(value INTEGER); WITH RECURSIVE sequence(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM sequence WHERE x<1000000) INSERT INTO unfinished SELECT x FROM sequence;",
        std::time::Duration::ZERO).unwrap_err();
    assert!(
        matches!(error, crate::Error::Database(rusqlite::Error::SqliteFailure(code, _)) if code.code == rusqlite::ErrorCode::OperationInterrupted)
    );
    assert!(!connection.table_exists(None, "unfinished").unwrap());
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM schema_migrations", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let count:i64=connection.query_row("WITH RECURSIVE sequence(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM sequence WHERE x<10000) SELECT count(*) FROM sequence", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 10000);
}

#[tokio::test]
async fn schema_one_upgrades_once_and_keeps_a_verified_before_image() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("runtime");
    fs::create_dir(&root).unwrap();
    let path = root.join("workflows.sqlite3");
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.execute_batch(crate::database::SCHEMA).unwrap();
        connection
            .execute(
                "INSERT INTO schema_migrations VALUES(1,?1)",
                [crate::database::hash(crate::database::SCHEMA.as_bytes())],
            )
            .unwrap();
        connection
            .pragma_update(None, "application_id", crate::database::APPLICATION_ID)
            .unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();
    }
    let store = Store::open(&root).unwrap();
    store
        .call(|db| {
            assert_eq!(
                db.connection
                    .pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))?,
                crate::database::SCHEMA_VERSION
            );
            assert_eq!(
                db.connection
                    .query_row("SELECT count(*) FROM schema_migrations", [], |r| r
                        .get::<_, i64>(0))?,
                i64::from(crate::database::SCHEMA_VERSION)
            );
            Ok(())
        })
        .await
        .unwrap();
    store.close().await.unwrap();
    let backups = fs::read_dir(&root)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .ends_with("sqlite3.backup")
        })
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let before = rusqlite::Connection::open(backups[0].path()).unwrap();
    assert_eq!(
        before
            .pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
            .unwrap(),
        1
    );
    let reopened = Store::open(&root).unwrap();
    reopened.close().await.unwrap();
    assert_eq!(
        fs::read_dir(root)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry
                .file_name()
                .to_string_lossy()
                .ends_with("sqlite3.backup"))
            .count(),
        1
    );
}

#[tokio::test]
async fn conflicting_import_is_atomic_and_preserves_original_metadata() {
    let (dir, store) = fixture(4).await;
    let path = dir.path().join("accounts.json");
    let original = fs::read(&path).unwrap();
    let mut registry: BrowserProfileRegistry = serde_json::from_slice(&original).unwrap();
    let mut other = registry.accounts[0].clone();
    other.id = "new-account".into();
    other.email_hint = "new@example.invalid".into();
    other.provider_account_id = Some("new-identity".into());
    other.profile_dir = dir.path().join("second-profile");
    other.cdp_port = 9225;
    registry.accounts.insert(0, other);
    registry.accounts[1].provider_account_id = Some("conflicting-identity".into());
    fs::write(&path, serde_json::to_vec(&registry).unwrap()).unwrap();
    assert_eq!(
        store.import_registry(path, 1).await.unwrap_err().code(),
        "migration-identity-conflict"
    );
    assert_eq!(store.accounts().await.unwrap().len(), 1);
    assert_eq!(store.accounts().await.unwrap()[0].capacity, 4);
    let backup = fs::read_dir(dir.path().join("runtime"))
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().ends_with(".json.backup"))
        .unwrap()
        .path();
    assert_eq!(fs::read(backup).unwrap(), original);
    store.close().await.unwrap();
}

#[tokio::test]
async fn constraints_reject_orphans_configuration_changes_and_invalid_transitions() {
    let (_dir, store) = fixture(10).await;
    let run = store
        .submit(request(5, "constraints"), ModelSelection::Current, 0)
        .await
        .unwrap();
    let lease = store.claim(0, 0).await.unwrap().unwrap();
    store
        .call(move |db| {
            assert!(db
                .connection
                .execute(
                    "UPDATE runs SET deadline_ms=999999999 WHERE id=?1",
                    [&run.id]
                )
                .is_err());
            assert!(db
                .connection
                .execute(
                    "UPDATE runs SET submissions=submission_limit+1 WHERE id=?1",
                    [&run.id]
                )
                .is_err());
            assert!(db
                .connection
                .execute(
                    "UPDATE attempts SET state='completed' WHERE id=?1",
                    [&lease.attempt_id]
                )
                .is_err());
            assert!(db
                .connection
                .execute(
                    "UPDATE tasks SET run_id='missing-parent' WHERE id=?1",
                    [&lease.task_id]
                )
                .is_err());
            assert!(db
                .connection
                .execute("UPDATE accounts SET capacity=0", [])
                .is_err());
            Ok(())
        })
        .await
        .unwrap();
    store.close().await.unwrap();
}

#[tokio::test]
async fn migration_drift_future_schema_and_corrupt_input_fail_closed() {
    let (dir, store) = fixture(10).await;
    store.close().await.unwrap();
    let root = dir.path().join("runtime");
    {
        let conn = rusqlite::Connection::open(root.join("workflows.sqlite3")).unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
    }
    assert_eq!(
        Store::open(&root).err().unwrap().code(),
        "schema-incompatible"
    );
    {
        let conn = rusqlite::Connection::open(root.join("workflows.sqlite3")).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute("UPDATE schema_migrations SET sha256=?1", ["0".repeat(64)])
            .unwrap();
    }
    assert_eq!(Store::open(&root).err().unwrap().code(), "migration-drift");
    let corrupt = dir.path().join("corrupt");
    fs::create_dir(&corrupt).unwrap();
    fs::write(corrupt.join("workflows.sqlite3"), b"not a sqlite database").unwrap();
    assert!(Store::open(corrupt).is_err());
}

#[tokio::test]
async fn incomplete_restore_and_bad_backup_hash_never_open_as_valid_runtime() {
    let (dir, store) = fixture(10).await;
    let backup = dir.path().join("backup");
    store.backup(backup.clone()).await.unwrap();
    fs::write(backup.join("workflows.sqlite3"), b"corrupt").unwrap();
    assert_eq!(
        Store::restore(&backup, &dir.path().join("restore"))
            .err()
            .unwrap()
            .code(),
        "backup-integrity"
    );
    assert!(!dir.path().join("restore").exists());
    let unfinished = dir.path().join("unfinished");
    fs::create_dir(&unfinished).unwrap();
    fs::write(unfinished.join("restore.incomplete"), b"interrupted").unwrap();
    assert_eq!(
        Store::open(unfinished).err().unwrap().code(),
        "restore-incomplete"
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn sqlite_disk_full_rolls_back_submission_and_events() {
    let (_dir, store) = fixture(10).await;
    store
        .call(|db| {
            let pages: i64 = db
                .connection
                .pragma_query_value(None, "page_count", |r| r.get(0))?;
            db.connection.pragma_update(None, "max_page_count", pages)?;
            Ok(())
        })
        .await
        .unwrap();
    let mut large = request(10, "disk-full");
    large.concept = "x".repeat(64_000);
    let error = store
        .submit(large, ModelSelection::Current, 0)
        .await
        .unwrap_err();
    match error {
        crate::Error::Database(rusqlite::Error::SqliteFailure(code, _)) => {
            assert_eq!(code.code, rusqlite::ErrorCode::DiskFull)
        }
        other => panic!("expected disk full, received {other}"),
    }
    assert!(store.runs().await.unwrap().is_empty());
    store
        .call(|db| {
            let count: i64 = db
                .connection
                .query_row("SELECT count(*) FROM events", [], |r| r.get(0))?;
            assert_eq!(count, 0);
            Ok(())
        })
        .await
        .unwrap();
    store.close().await.unwrap();
}

#[tokio::test]
async fn timeout_keeps_text_partial_and_never_advances_stage() {
    let (_dir, store) = fixture(10).await;
    let run = store
        .submit(request(5, "timeout"), ModelSelection::Current, 0)
        .await
        .unwrap();
    let lease = store.claim(0, 0).await.unwrap().unwrap();
    accept(&store, &lease, 0).await;
    let result = store
        .capture(lease.clone(), capture(&lease, 5), RESPONSE_TIMEOUT_MS + 1)
        .await
        .unwrap();
    assert_eq!(result.tasks[0].status, "partial");
    assert_eq!(
        result.tasks[0].error_code.as_deref(),
        Some("response-timeout")
    );
    assert!(result.artifacts.iter().all(|a| a.completion == "partial"));
    store
        .pause(run.id.clone(), RESPONSE_TIMEOUT_MS + 2)
        .await
        .unwrap();
    store
        .resume(run.id.clone(), false, RESPONSE_TIMEOUT_MS + 3)
        .await
        .unwrap();
    let retry = store
        .claim(RESPONSE_TIMEOUT_MS + 4, 0)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retry.position, 1);
    assert_ne!(retry.attempt_id, lease.attempt_id);
    assert_eq!(store.run(run.id).await.unwrap().submissions, 1);
    store.close().await.unwrap();
}

#[tokio::test]
async fn unconfirmed_stop_retains_partial_text_and_reservation() {
    let (_dir, store) = fixture(10).await;
    let run = store
        .submit(request(5, "unconfirmed-stop"), ModelSelection::Current, 0)
        .await
        .unwrap();
    let lease = store.claim(0, 0).await.unwrap().unwrap();
    accept(&store, &lease, 0).await;
    let mut partial = capture(&lease, 5);
    partial.complete = false;
    partial.error_code = Some("original-stream-error".into());
    let result = store
        .preserve_partial(lease.clone(), partial, 100)
        .await
        .unwrap();
    assert_eq!(result.status, "reconciliation-required");
    assert_eq!(result.tasks[0].status, "running");
    assert_eq!(
        result.tasks[0].error_code.as_deref(),
        Some("original-stream-error")
    );
    assert_eq!(result.artifacts.len(), 1);
    assert_eq!(result.artifacts[0].completion, "partial");
    assert_eq!(
        store.active_leases().await.unwrap()[0].reservation_id,
        lease.reservation_id
    );
    assert_eq!(
        store.attempts(run.id.clone()).await.unwrap()[0].state,
        "uncertain"
    );
    assert!(store.resume(run.id, false, 200).await.is_err());
    store.close().await.unwrap();
}

#[tokio::test]
async fn comparison_input_limit_never_truncates_evidence() {
    let (_dir, store) = fixture(10).await;
    let run = store
        .submit(request(5, "context"), ModelSelection::Current, 0)
        .await
        .unwrap();
    let task = run.tasks.iter().find(|t| t.stage == "compare").unwrap();
    let error = crate::prompts::stage_prompt(
        &run,
        task,
        serde_json::json!({"evidence":"x".repeat(STAGE_CHAR_LIMIT)}),
    )
    .unwrap_err();
    assert_eq!(error.code(), "context-too-large");
    store.close().await.unwrap();
}

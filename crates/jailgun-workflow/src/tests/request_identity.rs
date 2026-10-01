use super::*;

#[tokio::test]
async fn retries_preserve_snapshot_after_model_change_expiry_and_restart() {
    let (dir, store) = fixture(10).await;
    let original = store
        .submit(
            request(5, "stable"),
            ModelSelection::Specific {
                name: "original".into(),
            },
            10,
        )
        .await
        .unwrap();
    store
        .call(|db| {
            db.connection.execute(
                "UPDATE account_sessions SET model_json=NULL,lifecycle='expired'",
                [],
            )?;
            db.connection
                .execute("UPDATE accounts SET readiness='expired'", [])?;
            Ok(())
        })
        .await
        .unwrap();
    // Lookup precedes settings resolution, even with no currently confirmed model.
    let retry = store
        .submit_for_account(request(5, "stable"), 20)
        .await
        .unwrap();
    assert_eq!(retry.id, original.id);
    assert_eq!(
        serde_json::to_value(retry.configuration).unwrap(),
        serde_json::to_value(&original.configuration).unwrap()
    );
    assert_eq!(
        store
            .submit_for_account(request(5, "new"), 20)
            .await
            .unwrap_err()
            .code(),
        "model-selection-required"
    );
    let mut changed = request(5, "stable");
    changed.concept.push(' ');
    assert_eq!(
        store
            .submit_for_account(changed, 20)
            .await
            .unwrap_err()
            .code(),
        "idempotency-conflict"
    );
    store.close().await.unwrap();
    let store = Store::open(dir.path().join("runtime")).unwrap();
    let (a, b) = tokio::join!(
        store.submit(request(5, "stable"), ModelSelection::Current, 30),
        store.submit(
            request(5, "stable"),
            ModelSelection::Specific {
                name: "replacement".into()
            },
            30
        )
    );
    for retry in [a.unwrap(), b.unwrap()] {
        assert_eq!(retry.id, original.id);
        assert_eq!(retry.created_ms, 10);
        assert_eq!(
            serde_json::to_value(retry.configuration).unwrap(),
            serde_json::to_value(&original.configuration).unwrap()
        );
    }
    assert_eq!(store.runs().await.unwrap().len(), 1);
    store.close().await.unwrap();
}

#[tokio::test]
async fn simultaneous_new_requests_have_one_immutable_snapshot() {
    let (_dir, store) = fixture(10).await;
    let (a, b) = tokio::join!(
        store.submit(request(5, "concurrent"), ModelSelection::Current, 10),
        store.submit(
            request(5, "concurrent"),
            ModelSelection::Specific {
                name: "other".into()
            },
            10
        )
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.id, b.id);
    assert_eq!(
        serde_json::to_value(a.configuration).unwrap(),
        serde_json::to_value(b.configuration).unwrap()
    );
    assert_eq!(store.events(a.id, 0).await.unwrap().len(), 1);
    store.close().await.unwrap();
}

#[tokio::test]
async fn historical_identity_migration_preserves_hashes_and_rolls_back_conflicts() {
    for corrupt in [false, true] {
        let (dir, store) = fixture(10).await;
        let original = store
            .submit(
                request(5, "historical"),
                ModelSelection::Specific {
                    name: "retired".into(),
                },
                0,
            )
            .await
            .unwrap();
        let original_hash = store.call(move |db| {
            // Reconstruct the exact schema-4 database, retaining its immutable rows.
            db.connection.execute_batch("DROP TABLE request_identities; DELETE FROM schema_migrations WHERE version=5; PRAGMA user_version=4;")?;
            if corrupt {
                db.connection.execute_batch("DROP TRIGGER immutable_run_configuration; UPDATE runs SET request_hash=printf('%064d',0);")?;
            }
            Ok(db.connection.query_row("SELECT request_hash FROM runs", [], |r| r.get::<_,String>(0))?)
        }).await.unwrap();
        store.close().await.unwrap();
        let root = dir.path().join("runtime");
        if corrupt {
            assert_eq!(
                Store::open(&root).err().unwrap().code(),
                "request-identity-conflict"
            );
            let db = rusqlite::Connection::open(root.join("workflows.sqlite3")).unwrap();
            assert_eq!(
                db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
                    .unwrap(),
                4
            );
            assert!(!db.table_exists(None, "request_identities").unwrap());
        } else {
            let store = Store::open(&root).unwrap();
            assert_eq!(
                store
                    .submit_for_account(request(5, "historical"), 100)
                    .await
                    .unwrap()
                    .id,
                original.id
            );
            store
                .call(move |db| {
                    assert_eq!(
                        db.connection
                            .query_row("SELECT request_hash FROM runs", [], |r| r
                                .get::<_, String>(0))?,
                        original_hash
                    );
                    Ok(())
                })
                .await
                .unwrap();
            store.close().await.unwrap();
        }
    }
}

use super::*;

#[tokio::test]
async fn token_secrets_are_not_stored_and_revocation_survives_restart() {
    let (directory, store) = fixture(10).await;
    let created = store
        .issue_token(
            IssueToken {
                name: "Build client".into(),
                account_ids: vec!["test-account".into(), "test-account".into()],
            },
            100,
        )
        .await
        .unwrap();
    assert_eq!(created.metadata.account_ids, vec!["test-account"]);
    assert_eq!(created.secret.len(), 72);
    assert!(store
        .authenticate_token("wrong".into())
        .await
        .unwrap()
        .is_none());
    let secret = created.secret.clone();
    store
        .call(move |db| {
            let digest: String =
                db.connection
                    .query_row("SELECT digest FROM automation_tokens", [], |r| r.get(0))?;
            assert_eq!(digest, crate::database::hash(secret.as_bytes()));
            assert_ne!(digest, secret);
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(
        store
            .authenticate_token(created.secret.clone())
            .await
            .unwrap()
            .unwrap()
            .id,
        created.metadata.id
    );
    let listed = serde_json::to_string(&store.tokens().await.unwrap()).unwrap();
    assert!(!listed.contains(&created.secret));
    assert!(!listed.contains("digest"));
    assert!(store
        .issue_token(
            IssueToken {
                name: "Invalid scope".into(),
                account_ids: vec!["absent".into()]
            },
            101
        )
        .await
        .is_err());
    assert_eq!(store.tokens().await.unwrap().len(), 1);
    store.close().await.unwrap();
    let reopened = Store::open(directory.path().join("runtime")).unwrap();
    assert!(reopened
        .authenticate_token(created.secret.clone())
        .await
        .unwrap()
        .is_some());
    assert_eq!(
        reopened
            .revoke_token(created.metadata.id.clone(), 200)
            .await
            .unwrap()
            .revoked_ms,
        Some(200)
    );
    assert_eq!(
        reopened
            .revoke_token(created.metadata.id.clone(), 300)
            .await
            .unwrap()
            .revoked_ms,
        Some(200)
    );
    reopened
        .call(move |db| {
            assert!(db
                .connection
                .execute(
                    "UPDATE automation_tokens SET revoked_ms=NULL WHERE id=?1",
                    [created.metadata.id]
                )
                .is_err());
            Ok(())
        })
        .await
        .unwrap();
    reopened.close().await.unwrap();
    let reopened = Store::open(directory.path().join("runtime")).unwrap();
    assert!(reopened
        .authenticate_token(created.secret)
        .await
        .unwrap()
        .is_none());
}

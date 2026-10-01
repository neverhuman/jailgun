use super::*;

#[tokio::test]
async fn connection_preserves_profiles_requires_fresh_confirmation_and_records_model() {
    let (dir, store) = fixture(4).await;
    let account = store
        .connect_account(ConnectAccount {
            email: "person@example.invalid".into(),
            id: Some("person".into()),
        })
        .await
        .unwrap();
    assert_eq!(account.max_tabs, 10);
    assert_eq!(
        store
            .account_model(account.id.clone())
            .await
            .unwrap_err()
            .code(),
        "model-selection-required"
    );
    let imported = store
        .connect_account(ConnectAccount {
            email: "synthetic@example.invalid".into(),
            id: Some("test-account".into()),
        })
        .await
        .unwrap();
    assert_eq!(imported.max_tabs, 4);
    assert_eq!(imported.profile_dir, dir.path().join("original profile"));
    assert_ne!(account.cdp_port, imported.cdp_port);
    let observation = AccountObservation {
        identity: AccountIdentity {
            id: "new-person".into(),
            email: account.email_hint.clone(),
        },
        model: "Fixture Pro".into(),
        available_models: vec!["Fixture Pro".into(), "Fixture Fast".into()],
    };
    let confirmation = ConfirmAccount {
        identity: observation.identity.clone(),
        model: ModelSelection::Specific {
            name: "Fixture Fast".into(),
        },
    };
    assert!(!store
        .observe_account(account.id.clone(), observation.clone(), 100)
        .await
        .unwrap());
    assert!(store
        .confirm_account(account.id.clone(), confirmation.clone(), 101)
        .await
        .is_err());
    store
        .login_state(account.id.clone(), "verifying".into(), None, Some(900_000))
        .await
        .unwrap();
    assert_eq!(
        store
            .confirm_account(account.id.clone(), confirmation.clone(), 31_000)
            .await
            .unwrap_err()
            .code(),
        "account-verification-expired"
    );
    let mut unavailable = confirmation.clone();
    unavailable.model = ModelSelection::Specific {
        name: "Absent".into(),
    };
    assert_eq!(
        store
            .confirm_account(account.id.clone(), unavailable, 101)
            .await
            .unwrap_err()
            .code(),
        "model-unavailable"
    );
    store
        .login_state(account.id.clone(), "cancelled".into(), None, None)
        .await
        .unwrap();
    assert!(store
        .confirm_account(account.id.clone(), confirmation.clone(), 102)
        .await
        .is_err());
    store
        .login_state(account.id.clone(), "starting".into(), None, Some(900_000))
        .await
        .unwrap();
    store
        .login_state(account.id.clone(), "verifying".into(), None, Some(900_000))
        .await
        .unwrap();
    store
        .observe_account(account.id.clone(), observation.clone(), 102)
        .await
        .unwrap();
    store
        .confirm_account(account.id.clone(), confirmation.clone(), 103)
        .await
        .unwrap();
    assert_eq!(
        store.account_model(account.id.clone()).await.unwrap(),
        confirmation.model
    );
    let mut other = observation;
    other.identity.id = "different-person".into();
    assert_eq!(
        store
            .observe_account(account.id.clone(), other, 104)
            .await
            .unwrap_err()
            .code(),
        "account-mismatch"
    );
    let repeated = store
        .connect_account(ConnectAccount {
            email: account.email_hint.clone(),
            id: Some(account.id.clone()),
        })
        .await
        .unwrap();
    assert_eq!(repeated.profile_dir, account.profile_dir);
    assert_eq!(repeated.cdp_port, account.cdp_port);
    assert_eq!(repeated.provider_account_id.as_deref(), Some("new-person"));
    store.close().await.unwrap();
    let reopened = Store::open(dir.path().join("runtime")).unwrap();
    assert_eq!(
        reopened.account_model(account.id).await.unwrap(),
        confirmation.model
    );
    reopened.close().await.unwrap();
}

#[tokio::test]
async fn concurrent_connections_cannot_duplicate_ports_or_rebind_an_email() {
    let (_dir, store) = fixture(10).await;
    let mut jobs = Vec::new();
    for index in 0..5 {
        let store = store.clone();
        jobs.push(tokio::spawn(async move {
            store
                .connect_account(ConnectAccount {
                    email: format!("person-{index}@example.invalid"),
                    id: Some(format!("person-{index}")),
                })
                .await
                .unwrap()
        }));
    }
    let mut ports = std::collections::HashSet::new();
    for job in jobs {
        assert!(ports.insert(job.await.unwrap().cdp_port));
    }
    assert!(store
        .connect_account(ConnectAccount {
            email: "person-1@example.invalid".into(),
            id: Some("another-id".into())
        })
        .await
        .is_err());
    assert!(store
        .connect_account(ConnectAccount {
            email: "different@example.invalid".into(),
            id: Some("person-1".into())
        })
        .await
        .is_err());
}

#[tokio::test]
async fn account_registration_retries_after_commit_failure_without_replacing_profile_ownership() {
    let (dir, store) = fixture(4).await;
    // A deferred foreign key fails only at COMMIT, after the private profile marker exists.
    store.call(|db| {
        db.connection.execute_batch("CREATE TABLE registration_failure (account_id TEXT REFERENCES accounts(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER fail_registration AFTER INSERT ON account_sessions BEGIN INSERT INTO registration_failure VALUES ('not-registered'); END;")?;
        Ok(())
    }).await.unwrap();
    let request = ConnectAccount {
        email: "retry@example.invalid".into(),
        id: Some("retry".into()),
    };
    assert!(store.connect_account(request.clone()).await.is_err());
    let profile = dir.path().join("runtime/profiles/retry");
    let marker = profile.join(".jailgun-workflow-owner");
    let before = std::fs::read(&marker).unwrap();
    std::fs::write(
        profile.join("retained-profile-data"),
        "synthetic profile data",
    )
    .unwrap();
    assert!(!store
        .account_metadata()
        .await
        .unwrap()
        .iter()
        .any(|a| a.id == "retry"));
    store
        .call(|db| {
            db.connection.execute_batch(
                "DROP TRIGGER fail_registration; DROP TABLE registration_failure;",
            )?;
            Ok(())
        })
        .await
        .unwrap();
    let account = store.connect_account(request).await.unwrap();
    assert_eq!(account.id, "retry");
    assert_eq!(std::fs::read(marker).unwrap(), before);
    assert_eq!(
        std::fs::read_to_string(profile.join("retained-profile-data")).unwrap(),
        "synthetic profile data"
    );
    let foreign = dir.path().join("runtime/profiles/foreign");
    jailgun_core::browser_registry::ensure_private_dir(&foreign).unwrap();
    std::fs::write(
        foreign.join(".jailgun-workflow-owner"),
        "another installation\n",
    )
    .unwrap();
    let rejected = store
        .connect_account(ConnectAccount {
            email: "foreign@example.invalid".into(),
            id: Some("foreign".into()),
        })
        .await
        .unwrap_err();
    assert_eq!(rejected.code(), "profile-ownership-conflict");
    #[cfg(unix)]
    {
        let linked = dir.path().join("runtime/profiles/linked");
        jailgun_core::browser_registry::ensure_private_dir(&linked).unwrap();
        std::os::unix::fs::symlink(
            profile.join(".jailgun-workflow-owner"),
            linked.join(".jailgun-workflow-owner"),
        )
        .unwrap();
        assert_eq!(
            store
                .connect_account(ConnectAccount {
                    email: "linked@example.invalid".into(),
                    id: Some("linked".into())
                })
                .await
                .unwrap_err()
                .code(),
            "profile-ownership-conflict"
        );
    }
    assert!(!store
        .account_metadata()
        .await
        .unwrap()
        .iter()
        .any(|a| a.id == "foreign"));
    store.close().await.unwrap();
}

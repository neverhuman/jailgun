use super::*;
use jailgun_core::installation::Installation;
use jailgun_workflow::Store;
use std::fs;

#[tokio::test]
async fn offline_backup_restore_preserves_completed_results_and_never_overwrites() {
    let fixture = Fixture::new().await;
    let id = complete(&fixture).await;
    let expected = fixture.store.run(id.clone()).await.unwrap();
    let accounts = fixture.store.accounts().await.unwrap();
    fixture.store.close().await.unwrap();
    let runtime = fixture.dir.path().join("runtime");
    let owner = Installation::acquire(&runtime).unwrap();
    let credential = owner.operator_token().unwrap();
    let backup = fixture.dir.path().join("backup with spaces");
    let args = [
        "data",
        "backup",
        "--runtime",
        runtime.to_str().unwrap(),
        "--out",
        backup.to_str().unwrap(),
    ];
    failure(fixture.call(&args).await, "runtime-owned");
    assert!(!backup.exists());
    drop(owner);
    let report = success(fixture.call(&args).await);
    assert_eq!(report["operation"], "backup");
    assert_eq!(report["profiles_included"], false);
    assert!(!backup.join("operator-token").exists());
    assert!(!backup.join("profiles").exists());
    let database = fs::read(backup.join("workflows.sqlite3")).unwrap();
    failure(fixture.call(&args).await, "backup-exists");
    assert_eq!(
        database,
        fs::read(backup.join("workflows.sqlite3")).unwrap()
    );
    let destination = fixture.dir.path().join("restored runtime");
    let args = [
        "data",
        "restore",
        "--backup",
        backup.to_str().unwrap(),
        "--out",
        destination.to_str().unwrap(),
    ];
    let report = success(fixture.call(&args).await);
    assert_eq!(report["operation"], "restore");
    failure(fixture.call(&args).await, "restore-destination-exists");
    let restored = Store::open(&destination).unwrap();
    assert_eq!(
        serde_json::to_value(restored.run(id.clone()).await.unwrap()).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
    assert_eq!(
        serde_json::to_value(restored.accounts().await.unwrap()).unwrap(),
        serde_json::to_value(accounts).unwrap()
    );
    assert_eq!(restored.result(id).await.unwrap().status, "completed");
    for artifact in &expected.artifacts {
        let (_, bytes) = restored
            .artifact(expected.id.clone(), artifact.id.clone())
            .await
            .unwrap();
        assert!(!bytes.is_empty());
    }
    restored.close().await.unwrap();
    assert_eq!(
        fs::read_to_string(runtime.join("operator-token"))
            .unwrap()
            .trim(),
        credential
    );
    assert!(!destination.join("operator-token").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for root in [&backup, &destination] {
            assert_eq!(
                fs::metadata(root).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(root.join("workflows.sqlite3"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
    // A valid database with a damaged response must never produce an openable restore.
    fs::write(
        backup.join("artifacts").join(&expected.artifacts[0].id),
        "damaged",
    )
    .unwrap();
    let damaged = fixture.dir.path().join("damaged restore");
    failure(
        fixture
            .call(&[
                "data",
                "restore",
                "--backup",
                backup.to_str().unwrap(),
                "--out",
                damaged.to_str().unwrap(),
            ])
            .await,
        "artifact-integrity",
    );
    assert_eq!(
        Store::open(&damaged).err().unwrap().code(),
        "restore-incomplete"
    );
    let original = Store::open(&runtime).unwrap();
    assert_eq!(original.run(expected.id).await.unwrap().status, "completed");
    original.close().await.unwrap();
}

#[tokio::test]
async fn missing_runtime_and_writer_ownership_fail_without_creating_a_backup() {
    let fixture = Fixture::new().await;
    let missing = fixture.dir.path().join("missing runtime");
    let out = fixture.dir.path().join("backup");
    failure(
        fixture
            .call(&[
                "data",
                "backup",
                "--runtime",
                missing.to_str().unwrap(),
                "--out",
                out.to_str().unwrap(),
            ])
            .await,
        "runtime-not-found",
    );
    assert!(!missing.exists());
    assert!(!out.exists());
    let runtime = fixture.dir.path().join("runtime");
    failure(
        fixture
            .call(&[
                "data",
                "backup",
                "--runtime",
                runtime.to_str().unwrap(),
                "--out",
                out.to_str().unwrap(),
            ])
            .await,
        "database-owned",
    );
    assert!(!out.exists());
    fixture.store.close().await.unwrap();
}

async fn complete(fixture: &Fixture) -> String {
    let id = fixture.submit().await;
    let mut clock = now();
    for _ in 0..9 {
        let lease = fixture.store.claim(clock, 0).await.unwrap().unwrap();
        fixture
            .store
            .begin_submission(lease.clone(), clock)
            .await
            .unwrap();
        fixture
            .store
            .accepted(
                lease.clone(),
                AcceptedTurn {
                    conversation_id: lease.attempt_id.clone(),
                    conversation_url: format!("https://chatgpt.com/c/{}", lease.attempt_id),
                    user_turn_id: format!("user-{}", lease.attempt_id),
                    observed_model: "Fixture".into(),
                },
                clock + 1,
            )
            .await
            .unwrap();
        let capture = Capture {
            markdown: format!("# {}\n\nA synthetic stored response.", lease.stage),
            summary: (lease.stage == "explore").then(|| CandidateSummary {
                proposal: "A synthetic library".into(),
                strengths: vec!["Sharing".into()],
                weaknesses: vec!["Maintenance".into()],
                assumptions: vec!["Demand".into()],
                next_steps: vec!["Pilot".into()],
            }),
            evaluations: (lease.stage == "compare").then(|| {
                (1..=5)
                    .map(|candidate| CandidateEvaluation {
                        candidate,
                        scores: default_criteria()
                            .into_iter()
                            .map(|criterion| CriterionScore {
                                criterion: criterion.name,
                                score: 70,
                                rationale: "Synthetic judgment".into(),
                            })
                            .collect(),
                        disagreements: vec![],
                        uncertainty: "Synthetic evidence".into(),
                    })
                    .collect()
            }),
            conversation_id: lease.attempt_id.clone(),
            conversation_url: format!("https://chatgpt.com/c/{}", lease.attempt_id),
            observed_model: "Fixture".into(),
            complete: true,
            error_code: None,
        };
        fixture
            .store
            .capture(lease, capture, clock + 2)
            .await
            .unwrap();
        clock += 36_000;
    }
    assert_eq!(
        fixture.store.run(id.clone()).await.unwrap().status,
        "completed"
    );
    id
}

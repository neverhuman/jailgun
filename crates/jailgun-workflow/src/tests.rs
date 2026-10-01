use crate::{model::*, FailureDisposition, Store};
mod account_sessions;
mod artifact_reads;
mod request_identity;
mod scheduling;
mod storage;
mod tokens;
use jailgun_core::{BrowserAccount, BrowserAccountStatus, BrowserProfileRegistry};
use std::{fs, path::Path};

async fn fixture(capacity: u16) -> (tempfile::TempDir, Store) {
    let proofs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/workflow-unit-proofs");
    jailgun_core::browser_registry::ensure_private_dir(&proofs).unwrap();
    let directory = tempfile::tempdir_in(proofs).unwrap();
    let root = directory.path().join("runtime");
    let store = Store::open(&root).unwrap();
    import(&store, directory.path(), capacity).await;
    (directory, store)
}

async fn import(store: &Store, root: &Path, capacity: u16) {
    let registry = BrowserProfileRegistry {
        version: 1,
        accounts: vec![BrowserAccount {
            id: "test-account".into(),
            email_hint: "synthetic@example.invalid".into(),
            profile_dir: root.join("original profile"),
            state_dir: root.join("browser-state"),
            downloads_dir: root.join("downloads"),
            cdp_port: 9224,
            max_tabs: capacity,
            status: BrowserAccountStatus::Ready,
            last_verified_at: Some("fixture".into()),
            provider_account_id: Some("synthetic-account".into()),
        }],
    };
    let path = root.join("accounts.json");
    fs::write(&path, serde_json::to_vec(&registry).unwrap()).unwrap();
    assert_eq!(store.import_registry(path.clone(), 0).await.unwrap(), 1);
    assert_eq!(store.import_registry(path, 1).await.unwrap(), 0);
}

fn request(count: u16, key: &str) -> ConceptRequest {
    ConceptRequest {
        concept: "A synthetic neighborhood lending library".into(),
        account_id: "test-account".into(),
        candidate_count: count,
        constraints: "A small operating budget".into(),
        criteria: default_criteria(),
        idempotency_key: key.into(),
    }
}

fn summary() -> CandidateSummary {
    CandidateSummary {
        proposal: "A testable lending library".into(),
        strengths: vec!["Shared value".into()],
        weaknesses: vec!["Maintenance cost".into()],
        assumptions: vec!["Demand needs testing".into()],
        next_steps: vec!["Pilot with volunteers".into()],
    }
}

async fn accept(store: &Store, lease: &Lease, now: i64) {
    store.begin_submission(lease.clone(), now).await.unwrap();
    store
        .accepted(
            lease.clone(),
            AcceptedTurn {
                conversation_id: lease.attempt_id.clone(),
                conversation_url: format!("https://chatgpt.com/c/{}", lease.attempt_id),
                user_turn_id: format!("user-{}", lease.attempt_id),
                observed_model: "fixture-model".into(),
            },
            now + 1,
        )
        .await
        .unwrap();
}

fn capture(lease: &Lease, count: u16) -> Capture {
    Capture { markdown:format!("# {}\n\n- A useful result\n\n```rust\nfn main() {{}}\n```\n\n[Evidence](https://example.invalid/evidence)",lease.stage),
        summary:(lease.stage=="explore").then(summary),
        evaluations:(lease.stage=="compare").then(||(1..=count).map(|candidate|CandidateEvaluation { candidate,scores:default_criteria().into_iter().map(|c|CriterionScore {criterion:c.name,score:80,rationale:"A model judgment for this fixture".into()}).collect(),disagreements:vec![],uncertainty:"No independent evidence in this synthetic test".into() }).collect()),
        conversation_id:lease.attempt_id.clone(),conversation_url:format!("https://chatgpt.com/c/{}",lease.attempt_id),observed_model:"fixture-model".into(),complete:true,error_code:None }
}

#[tokio::test]
async fn sqlite_version_lock_schema_and_private_permissions() {
    let (dir, store) = fixture(10).await;
    println!("bundled SQLite {}", store.sqlite_version().await.unwrap());
    assert!(rusqlite::version_number() >= 3_051_003);
    assert_eq!(
        Store::open(dir.path().join("runtime"))
            .err()
            .unwrap()
            .code(),
        "database-owned"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(dir.path().join("runtime"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(dir.path().join("runtime/workflows.sqlite3"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    store.close().await.unwrap();
    let reopened = Store::open(dir.path().join("runtime")).unwrap();
    assert_eq!(reopened.accounts().await.unwrap()[0].capacity, 10);
    reopened.close().await.unwrap();
}

#[tokio::test]
async fn idempotency_pacing_capacity_and_separate_releases() {
    let (_dir, store) = fixture(2).await;
    let first = store
        .submit(request(5, "first"), ModelSelection::Current, 0)
        .await
        .unwrap();
    assert_eq!(
        store
            .submit(request(5, "first"), ModelSelection::Current, 1)
            .await
            .unwrap()
            .id,
        first.id
    );
    assert_eq!(
        store
            .submit(request(6, "first"), ModelSelection::Current, 1)
            .await
            .unwrap_err()
            .code(),
        "idempotency-conflict"
    );
    let second = store
        .submit(request(5, "second"), ModelSelection::Current, 1)
        .await
        .unwrap();
    let a = store.claim(0, 0).await.unwrap().unwrap();
    assert!(store.claim(0, 0).await.unwrap().is_none());
    store.pause(first.id.clone(), 1).await.unwrap();
    let b = store.claim(30_000, 0).await.unwrap().unwrap();
    assert_eq!(b.run_id, second.id);
    assert_ne!(a.reservation_id, b.reservation_id);
    assert!(store.claim(60_000, 0).await.unwrap().is_none());
    let mut wrong = a.clone();
    wrong.run_id = second.id.clone();
    assert_eq!(
        store
            .fail_attempt(
                wrong,
                "test".into(),
                FailureDisposition::DefinitelyNotAccepted,
                60_000
            )
            .await
            .unwrap_err()
            .code(),
        "ownership-mismatch"
    );
    store.cancel(first.id.clone(), 60_001).await.unwrap();
    store.cancel(first.id, 60_002).await.unwrap();
    assert_eq!(store.accounts().await.unwrap()[0].active, 2);
    store.acknowledge_cancel(a.clone(), 60_003).await.unwrap();
    store.acknowledge_cancel(a, 60_004).await.unwrap();
    assert_eq!(store.accounts().await.unwrap()[0].active, 1);
    assert_eq!(
        store.active_leases().await.unwrap()[0].attempt_id,
        b.attempt_id
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn five_and_ten_candidate_pipelines_survive_backup_and_restart() {
    for count in [5, 10] {
        let (dir, store) = fixture(10).await;
        let run = store
            .submit(request(count, "full"), ModelSelection::Current, 0)
            .await
            .unwrap();
        let mut stages = Vec::new();
        for index in 0..count + 4 {
            let now = i64::from(index) * 35_000;
            let lease = store.claim(now, 0).await.unwrap().unwrap();
            let prompt = store.prompt(lease.clone()).await.unwrap();
            assert!(prompt.contains("input data"));
            assert!(prompt.chars().count() <= STAGE_CHAR_LIMIT);
            accept(&store, &lease, now).await;
            store
                .capture(lease.clone(), capture(&lease, count), now + 100)
                .await
                .unwrap();
            stages.push(lease.stage);
        }
        assert_eq!(
            &stages[usize::from(count)..],
            ["compare", "synthesize", "critique", "revise"]
        );
        let complete = store.run(run.id.clone()).await.unwrap();
        assert_eq!(complete.status, "completed");
        assert_eq!(complete.submissions, count + 4);
        assert_eq!(
            complete.configuration.perspectives.len(),
            usize::from(count)
        );
        let final_artifact = complete
            .artifacts
            .iter()
            .find(|a| a.name == "final.md")
            .unwrap();
        let (_, bytes) = store
            .artifact(run.id.clone(), final_artifact.id.clone())
            .await
            .unwrap();
        assert!(String::from_utf8(bytes).unwrap().contains("```rust"));
        assert_eq!(
            store
                .result(run.id.clone())
                .await
                .unwrap()
                .final_artifact
                .unwrap()
                .id,
            final_artifact.id
        );
        let events = store.events(run.id.clone(), 0).await.unwrap();
        assert!(events.windows(2).all(|w| w[0].sequence < w[1].sequence));
        let after = events[events.len() - 2].sequence;
        assert_eq!(store.events(run.id.clone(), after).await.unwrap().len(), 1);
        let backup = dir.path().join("backup");
        store.backup(backup.clone()).await.unwrap();
        store.close().await.unwrap();
        let restored = Store::restore(&backup, &dir.path().join("restored runtime")).unwrap();
        assert_eq!(
            restored.result(run.id.clone()).await.unwrap().status,
            "completed"
        );
        assert_eq!(restored.run(run.id).await.unwrap().submissions, count + 4);
        restored.close().await.unwrap();
    }
}

#[tokio::test]
async fn restart_never_repeats_an_uncertain_submission() {
    let (dir, store) = fixture(10).await;
    let run = store
        .submit(request(5, "crash"), ModelSelection::Current, 0)
        .await
        .unwrap();
    let safe = store.claim(0, 0).await.unwrap().unwrap();
    let uncertain = store.claim(30_000, 0).await.unwrap().unwrap();
    store
        .begin_submission(uncertain.clone(), 30_000)
        .await
        .unwrap();
    let accepted = store.claim(60_000, 0).await.unwrap().unwrap();
    accept(&store, &accepted, 60_000).await;
    store.close().await.unwrap();
    let restarted = Store::open(dir.path().join("runtime")).unwrap();
    let recovery = restarted.recover(90_000).await.unwrap();
    assert_eq!(recovery.len(), 2);
    assert!(!recovery.iter().any(|r| r.attempt_id == safe.attempt_id));
    assert_eq!(
        restarted.run(run.id.clone()).await.unwrap().status,
        "reconciliation-required"
    );
    assert_eq!(restarted.run(run.id.clone()).await.unwrap().submissions, 2);
    assert!(restarted.claim(100_000, 0).await.unwrap().is_none());
    assert!(restarted
        .resume(run.id.clone(), false, 100_000)
        .await
        .is_err());
    assert!(restarted
        .reattached(uncertain, "unproven".into(), 100_000)
        .await
        .is_err());
    restarted
        .reattached(accepted.clone(), accepted.attempt_id.clone(), 100_000)
        .await
        .unwrap();
    assert!(!restarted
        .resume_reattached(run.id.clone(), vec![accepted.attempt_id.clone()], 100_000)
        .await
        .unwrap());
    assert_eq!(
        restarted.run(run.id.clone()).await.unwrap().status,
        "reconciliation-required"
    );
    restarted
        .capture(accepted.clone(), capture(&accepted, 5), 100_001)
        .await
        .unwrap();
    assert_eq!(
        restarted
            .run(run.id)
            .await
            .unwrap()
            .tasks
            .iter()
            .filter(|t| t.status == "completed")
            .count(),
        1
    );
    restarted.close().await.unwrap();
}

#[tokio::test]
async fn accepted_recovery_requires_all_turns_and_preserves_operator_pause() {
    for paused in [false, true] {
        let (_dir, store) = fixture(10).await;
        let run = store
            .submit(request(5, "reattach"), ModelSelection::Current, 0)
            .await
            .unwrap();
        let first = store.claim(0, 0).await.unwrap().unwrap();
        accept(&store, &first, 0).await;
        let second = store.claim(35_000, 0).await.unwrap().unwrap();
        accept(&store, &second, 35_000).await;
        if paused {
            store.pause(run.id.clone(), 40_000).await.unwrap();
        }
        assert_eq!(store.recover(50_000).await.unwrap().len(), 2);
        assert!(!store
            .resume_reattached(run.id.clone(), vec![first.attempt_id.clone()], 50_001)
            .await
            .unwrap());
        assert!(store
            .resume_reattached(
                run.id.clone(),
                vec![first.attempt_id, second.attempt_id],
                50_002
            )
            .await
            .unwrap());
        let recovered = store.run(run.id).await.unwrap();
        assert_eq!(recovered.status, if paused { "paused" } else { "queued" });
        assert_eq!(recovered.submissions, 2);
        assert_eq!(store.accounts().await.unwrap()[0].active, 2);
        store.close().await.unwrap();
    }
}

#[tokio::test]
async fn partials_keep_original_error_and_subset_requires_explicit_action() {
    let (_dir, store) = fixture(10).await;
    let run = store
        .submit(request(5, "subset"), ModelSelection::Current, 0)
        .await
        .unwrap();
    for index in 0..5 {
        let now = index * 35_000;
        let lease = store.claim(now, 0).await.unwrap().unwrap();
        accept(&store, &lease, now).await;
        let mut response = capture(&lease, 5);
        if index >= 3 {
            response.complete = false;
            response.error_code = Some("stream-interrupted".into());
        }
        store.capture(lease, response, now + 100).await.unwrap();
    }
    assert!(store.claim(200_000, 0).await.unwrap().is_none());
    let paused = store.run(run.id.clone()).await.unwrap();
    assert_eq!(paused.status, "paused");
    assert_eq!(
        paused
            .tasks
            .iter()
            .filter(
                |t| t.status == "partial" && t.error_code.as_deref() == Some("stream-interrupted")
            )
            .count(),
        2
    );
    assert!(store.result(run.id.clone()).await.is_err());
    store.resume(run.id.clone(), true, 200_000).await.unwrap();
    for index in 0..4 {
        let now = 200_000 + index * 35_000;
        let lease = store.claim(now, 0).await.unwrap().unwrap();
        accept(&store, &lease, now).await;
        store
            .capture(lease.clone(), capture(&lease, 3), now + 100)
            .await
            .unwrap();
    }
    let result = store.result(run.id).await.unwrap();
    assert_eq!(result.excluded_candidates, vec![4, 5]);
    store.close().await.unwrap();
}

#[tokio::test]
async fn invalid_summaries_retry_once_and_budgets_never_reset() {
    let (dir, store) = fixture(10).await;
    let run = store
        .submit(request(5, "retry"), ModelSelection::Current, 0)
        .await
        .unwrap();
    for index in 0..2 {
        let now = index * 35_000;
        let lease = store.claim(now, 0).await.unwrap().unwrap();
        assert_eq!(lease.position, 1);
        accept(&store, &lease, now).await;
        let mut response = capture(&lease, 5);
        response.summary = None;
        store.capture(lease, response, now + 100).await.unwrap();
    }
    let snapshot = store.run(run.id.clone()).await.unwrap();
    assert_eq!(snapshot.tasks[0].attempts, 2);
    assert_ne!(snapshot.tasks[0].status, "completed");
    assert_eq!(
        snapshot.tasks[0].error_code.as_deref(),
        Some("invalid-summary")
    );
    assert_eq!(snapshot.submissions, 2);
    store.close().await.unwrap();
    let restarted = Store::open(dir.path().join("runtime")).unwrap();
    assert_eq!(restarted.run(run.id).await.unwrap().submissions, 2);
    let next = restarted.claim(70_000, 0).await.unwrap().unwrap();
    assert_eq!(next.position, 2);
    restarted.close().await.unwrap();
}

#[tokio::test]
async fn delayed_submission_still_obeys_account_wide_pacing() {
    let (_dir, store) = fixture(10).await;
    store
        .submit(request(5, "pace"), ModelSelection::Current, 0)
        .await
        .unwrap();
    let first = store.claim(0, 5_000).await.unwrap().unwrap();
    let second = store.claim(35_000, 0).await.unwrap().unwrap();
    store.begin_submission(first, 35_000).await.unwrap();
    assert_eq!(
        store
            .begin_submission(second.clone(), 35_000)
            .await
            .unwrap_err()
            .code(),
        "submission-paced"
    );
    assert_eq!(
        store
            .begin_submission(second.clone(), 69_999)
            .await
            .unwrap_err()
            .code(),
        "submission-paced"
    );
    store.begin_submission(second, 70_000).await.unwrap();
    store.close().await.unwrap();
}

#[tokio::test]
async fn expiry_cooldown_and_identity_block_all_runs() {
    let (_dir, store) = fixture(10).await;
    let first = store
        .submit(request(5, "first"), ModelSelection::Current, 0)
        .await
        .unwrap();
    let second = store
        .submit(request(5, "second"), ModelSelection::Current, 1)
        .await
        .unwrap();
    let identity = jailgun_core::ProviderIdentity {
        id: "synthetic-account".into(),
        email: "synthetic@example.invalid".into(),
    };
    store
        .rate_limited("test-account".into(), None, 0)
        .await
        .unwrap();
    assert!(store.claim(299_999, 0).await.unwrap().is_none());
    assert!(store
        .verify_account("test-account".into(), identity.clone(), 299_999, false)
        .await
        .is_err());
    // Reaching the cooldown time alone does not restore capacity.
    assert!(store.claim(300_001, 0).await.unwrap().is_none());
    store
        .verify_account("test-account".into(), identity.clone(), 300_001, false)
        .await
        .unwrap();
    store
        .rate_limited("test-account".into(), Some(301_000), 300_002)
        .await
        .unwrap();
    assert_eq!(store.accounts().await.unwrap()[0].readiness, "paused");
    assert!(store
        .verify_account("test-account".into(), identity.clone(), 301_000, false)
        .await
        .is_err());
    store
        .verify_account("test-account".into(), identity.clone(), 301_000, true)
        .await
        .unwrap();
    store
        .account_expired("test-account".into(), 301_001)
        .await
        .unwrap();
    for id in [&first.id, &second.id] {
        assert_eq!(
            store.run(id.clone()).await.unwrap().status,
            "waiting-for-auth"
        );
    }
    assert!(store.claim(400_000, 0).await.unwrap().is_none());
    let mut wrong = identity.clone();
    wrong.id = "different-account".into();
    assert_eq!(
        store
            .verify_account("test-account".into(), wrong, 400_001, true)
            .await
            .unwrap_err()
            .code(),
        "account-mismatch"
    );
    store
        .verify_account("test-account".into(), identity, 400_002, true)
        .await
        .unwrap();
    assert!(store.claim(400_003, 0).await.unwrap().is_some());
    store.close().await.unwrap();
}

#[tokio::test]
async fn write_failure_does_not_complete_or_release_attempt_and_tampering_is_detected() {
    let (dir, store) = fixture(10).await;
    let run = store
        .submit(request(5, "io"), ModelSelection::Current, 0)
        .await
        .unwrap();
    let lease = store.claim(0, 0).await.unwrap().unwrap();
    accept(&store, &lease, 0).await;
    let root = dir.path().join("runtime/artifacts");
    let preserved = dir.path().join("preserved-artifacts");
    fs::rename(&root, &preserved).unwrap();
    fs::write(&root, "synthetic file blocking directory").unwrap();
    assert!(store
        .capture(lease.clone(), capture(&lease, 5), 100)
        .await
        .is_err());
    let blocked = store.run(run.id.clone()).await.unwrap();
    assert_eq!(blocked.tasks[0].status, "running");
    assert!(blocked.artifacts.is_empty());
    assert_eq!(store.accounts().await.unwrap()[0].active, 1);
    fs::remove_file(&root).unwrap();
    fs::rename(preserved, &root).unwrap();
    let done = store
        .capture(lease.clone(), capture(&lease, 5), 101)
        .await
        .unwrap();
    let artifact = done
        .artifacts
        .iter()
        .find(|a| a.name.ends_with(".md"))
        .unwrap();
    assert!(store
        .artifact(run.id.clone(), "../../operator-token".into())
        .await
        .is_err());
    fs::write(root.join(&artifact.id), "tampered").unwrap();
    assert_eq!(
        store
            .artifact(run.id, artifact.id.clone())
            .await
            .unwrap_err()
            .code(),
        "artifact-integrity"
    );
    store.close().await.unwrap();
}

#[test]
fn context_and_evaluation_reject_invalid_or_missing_evidence() {
    let mut too_large = summary();
    too_large.proposal = "界".repeat(6_001);
    assert_eq!(
        too_large.validate().unwrap_err().code(),
        "context-too-large"
    );
    assert!(crate::prompts::rank(&[], &default_criteria(), &[1, 2, 3]).is_err());
    let mut invalid = request(5, "invalid");
    invalid.criteria[0].weight = 31;
    assert!(invalid.validate().is_err());
    assert_eq!(
        crate::prompts::PERSPECTIVES
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        10
    );
}

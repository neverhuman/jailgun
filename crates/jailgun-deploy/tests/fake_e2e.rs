//! End-to-end integration test that drives `deploy_remote` against the fake
//! backends from `crate::fake`. This is the layer that the CI `e2e.sh` lane
//! shells out to once the CLI factory wires up `JAILGUN_FAKE_REMOTE=1`.
//!
//! Gated by the `fake-backends` feature so production builds never link this
//! test binary.

#![cfg(feature = "fake-backends")]

use std::path::PathBuf;

use jailgun_core::CleanupPolicy;
use jailgun_deploy::{
    deploy::{deploy_remote, DeployOutcome, DeployRequest},
    fake::{FakeCiTracker, FakeOutcome, FakeReceiptWriter, FakeRemoteJob, FakeRemoteUpload},
};
use tempfile::TempDir;
use tokio::sync::broadcast;

async fn make_archive(dir: &TempDir) -> (PathBuf, String) {
    let path = dir.path().join("source.tar.gz");
    tokio::fs::write(&path, b"fake source archive payload")
        .await
        .unwrap();
    let sha = jailgun_core::sha256_file(&path).unwrap();
    (path, sha)
}

fn request(archive: PathBuf, receipt_dir: PathBuf, dry_run: bool) -> DeployRequest {
    DeployRequest {
        run_id: "run-e2e".into(),
        tab_id: 1,
        remote_host: "fake-host".into(),
        remote_dir: "/srv/fake".into(),
        remote_command: "bash ci-fast-push.sh".into(),
        remote_archive_basename: "source.tar.gz".into(),
        local_archive_path: archive,
        strip_components: 1,
        cleanup_policy: CleanupPolicy::PreserveReset,
        receipt_dir,
        status_poll_seconds: 0,
        status_max_minutes: 1,
        ci_tracker_enabled: true,
        ci_branch: "main".into(),
        ci_max_attempts: 1,
        ci_poll_seconds: 0,
        stash_on_failure: true,
        dry_run,
    }
}

#[tokio::test]
async fn success_path_writes_receipt_and_emits_deploy_finished() {
    let dir = TempDir::new().unwrap();
    let (archive, sha) = make_archive(&dir).await;
    std::env::set_var("JAILGUN_FAKE_LOCAL_SHA", &sha);

    let mut upload = FakeRemoteUpload::new(FakeOutcome::Success);
    let mut job = FakeRemoteJob::new(FakeOutcome::Success);
    let mut ci = FakeCiTracker::new(FakeOutcome::Success);
    let mut writer = FakeReceiptWriter::new(dir.path().join("receipts"));
    let (tx, mut rx) = broadcast::channel(64);

    let receipt = deploy_remote(
        &mut upload,
        &mut job,
        &mut ci,
        &mut writer,
        request(archive, dir.path().join("receipts"), false),
        &tx,
    )
    .await
    .expect("deploy ok");

    assert_eq!(receipt.outcome, DeployOutcome::Succeeded);
    assert!(receipt.receipt_path.is_some());

    let receipt_path = receipt.receipt_path.as_ref().unwrap();
    assert!(tokio::fs::metadata(receipt_path).await.is_ok());

    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    assert!(!events.is_empty());

    std::env::remove_var("JAILGUN_FAKE_LOCAL_SHA");
}

#[tokio::test]
async fn ci_failure_records_succeeded_ci_failed_outcome() {
    let dir = TempDir::new().unwrap();
    let (archive, sha) = make_archive(&dir).await;
    std::env::set_var("JAILGUN_FAKE_LOCAL_SHA", &sha);

    let mut upload = FakeRemoteUpload::new(FakeOutcome::CiFail);
    let mut job = FakeRemoteJob::new(FakeOutcome::CiFail);
    let mut ci = FakeCiTracker::new(FakeOutcome::CiFail);
    let mut writer = FakeReceiptWriter::new(dir.path().join("receipts"));
    let (tx, _rx) = broadcast::channel(64);

    let receipt = deploy_remote(
        &mut upload,
        &mut job,
        &mut ci,
        &mut writer,
        request(archive, dir.path().join("receipts"), false),
        &tx,
    )
    .await
    .expect("deploy ok despite ci failure");

    assert_eq!(receipt.outcome, DeployOutcome::SucceededCiFailed);

    std::env::remove_var("JAILGUN_FAKE_LOCAL_SHA");
}

#[tokio::test]
async fn command_failure_records_failed_preserved_outcome() {
    let dir = TempDir::new().unwrap();
    let (archive, sha) = make_archive(&dir).await;
    std::env::set_var("JAILGUN_FAKE_LOCAL_SHA", &sha);

    let mut upload = FakeRemoteUpload::new(FakeOutcome::CommandFail);
    let mut job = FakeRemoteJob::new(FakeOutcome::CommandFail);
    let mut ci = FakeCiTracker::new(FakeOutcome::CommandFail);
    let mut writer = FakeReceiptWriter::new(dir.path().join("receipts"));
    let (tx, _rx) = broadcast::channel(64);

    let receipt = deploy_remote(
        &mut upload,
        &mut job,
        &mut ci,
        &mut writer,
        request(archive, dir.path().join("receipts"), false),
        &tx,
    )
    .await
    .expect("deploy ok despite command failure");

    assert_eq!(receipt.outcome, DeployOutcome::FailedPreserved);
    assert_eq!(
        receipt.final_status.preserved_ref.as_deref(),
        Some("jailgun-failed/fake-tab-01")
    );

    std::env::remove_var("JAILGUN_FAKE_LOCAL_SHA");
}

#[tokio::test]
async fn dry_run_outcome_is_dry_run_staged() {
    let dir = TempDir::new().unwrap();
    let (archive, sha) = make_archive(&dir).await;
    std::env::set_var("JAILGUN_FAKE_LOCAL_SHA", &sha);

    let mut upload = FakeRemoteUpload::new(FakeOutcome::Success);
    let mut job = FakeRemoteJob::new(FakeOutcome::Success);
    let mut ci = FakeCiTracker::new(FakeOutcome::Success);
    let mut writer = FakeReceiptWriter::new(dir.path().join("receipts"));
    let (tx, _rx) = broadcast::channel(64);

    let receipt = deploy_remote(
        &mut upload,
        &mut job,
        &mut ci,
        &mut writer,
        request(archive, dir.path().join("receipts"), true),
        &tx,
    )
    .await
    .expect("dry run ok");

    assert_eq!(receipt.outcome, DeployOutcome::DryRunStaged);

    std::env::remove_var("JAILGUN_FAKE_LOCAL_SHA");
}

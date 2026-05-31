mod support;

use jailgun_deploy::{deploy_remote, DeployOutcome, JobPhase, JobStatus};
use support::{archive_with_matching_upload, ci_writer_events, fake_deploy_request, FakeJob};

#[tokio::test]
async fn failed_preserved_records_outcome_with_preserved_refs() {
    let (_dir, archive, mut upload) = archive_with_matching_upload().await;
    let mut job = FakeJob::new(vec![JobStatus {
        phase: JobPhase::FailedPreserved,
        exit_code: Some(23),
        pre_head: Some("abc".into()),
        post_head: Some("def".into()),
        preserved_ref: Some("jailgun-failed/run-test-tab-01".into()),
        preserved_stash_ref: Some("jailgun-failed/run-test-tab-01-stash".into()),
        failure_reason: Some("remote-command-failed".into()),
        reset_ok: Some(true),
        ..Default::default()
    }]);
    let (mut ci, mut writer, tx, _rx) = ci_writer_events();
    let receipt = deploy_remote(
        &mut upload,
        &mut job,
        &mut ci,
        &mut writer,
        fake_deploy_request(archive),
        &tx,
    )
    .await
    .expect("preserved outcome is not an Err");
    assert_eq!(receipt.outcome, DeployOutcome::FailedPreserved);
    assert_eq!(
        receipt.final_status.preserved_ref.as_deref(),
        Some("jailgun-failed/run-test-tab-01")
    );
    assert_eq!(
        receipt.final_status.preserved_stash_ref.as_deref(),
        Some("jailgun-failed/run-test-tab-01-stash")
    );
}

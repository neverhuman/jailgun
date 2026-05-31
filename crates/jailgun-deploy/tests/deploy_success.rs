mod support;

use jailgun_deploy::{deploy_remote, DeployOutcome, JobPhase, JobStatus};
use support::{archive_with_matching_upload, ci_writer_events, fake_deploy_request, FakeJob};

#[tokio::test]
async fn success_path_records_succeeded_ci_skipped() {
    let (_dir, archive, mut upload) = archive_with_matching_upload().await;
    let mut job = FakeJob::new(vec![JobStatus {
        phase: JobPhase::Done,
        exit_code: Some(0),
        pre_head: Some("abc".into()),
        post_head: Some("abc".into()),
        ..Default::default()
    }]);
    let (mut ci, mut writer, tx, _rx) = ci_writer_events();
    let mut req = fake_deploy_request(archive);
    req.status_poll_seconds = 1;
    let receipt = deploy_remote(&mut upload, &mut job, &mut ci, &mut writer, req, &tx)
        .await
        .expect("success");
    assert_eq!(receipt.outcome, DeployOutcome::SucceededCiSkipped);
}

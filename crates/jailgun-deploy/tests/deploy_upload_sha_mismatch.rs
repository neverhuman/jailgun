mod support;

use jailgun_deploy::{deploy_remote, DeployError};
use support::{ci_writer_events, fake_deploy_request, make_archive, FakeJob, FakeUpload};

#[tokio::test]
async fn sha_mismatch_returns_err_and_removes_remote_file() {
    let (_dir, archive, _sha) = make_archive().await;
    let mut upload = FakeUpload::new(vec!["different".repeat(8)]);
    let mut job = FakeJob::new(vec![]);
    let (mut ci, mut writer, tx, _rx) = ci_writer_events();
    let err = deploy_remote(
        &mut upload,
        &mut job,
        &mut ci,
        &mut writer,
        fake_deploy_request(archive),
        &tx,
    )
    .await
    .expect_err("mismatch");
    assert!(matches!(err, DeployError::ShaMismatch { .. }));
    assert_eq!(upload.remove_calls, 1);
    assert!(writer.receipts.is_empty());
}

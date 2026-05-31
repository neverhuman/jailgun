mod support;

use jailgun_deploy::{deploy_remote, DeployOutcome};
use support::{archive_with_matching_upload, ci_writer_events, fake_deploy_request, FakeJob};

#[tokio::test]
async fn dry_run_emits_dry_run_staged_outcome() {
    let (_dir, archive, mut upload) = archive_with_matching_upload().await;
    let mut job = FakeJob::new(vec![]);
    let (mut ci, mut writer, tx, _rx) = ci_writer_events();
    let mut req = fake_deploy_request(archive);
    req.dry_run = true;
    let receipt = deploy_remote(&mut upload, &mut job, &mut ci, &mut writer, req, &tx)
        .await
        .expect("dry run ok");
    assert_eq!(receipt.outcome, DeployOutcome::DryRunStaged);
    assert!(job.install_called);
    assert!(!job.start_called);
    assert_eq!(writer.receipts.len(), 1);
}

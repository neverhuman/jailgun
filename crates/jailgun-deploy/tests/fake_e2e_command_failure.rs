#![cfg(feature = "fake-backends")]

mod support;

use jailgun_deploy::{fake::FakeOutcome, DeployOutcome};
use support::fake_e2e::deploy_with_fake_outcome;

#[tokio::test]
async fn command_failure_records_failed_preserved_outcome() {
    let (_dir, receipt, _events) = deploy_with_fake_outcome(FakeOutcome::CommandFail, false).await;

    assert_eq!(receipt.outcome, DeployOutcome::FailedPreserved);
    assert_eq!(
        receipt.final_status.preserved_ref.as_deref(),
        Some("jailgun-failed/fake-tab-01")
    );
}

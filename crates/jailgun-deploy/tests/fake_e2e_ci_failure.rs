#![cfg(feature = "fake-backends")]

mod support;

use jailgun_deploy::{fake::FakeOutcome, DeployOutcome};
use support::fake_e2e::deploy_with_fake_outcome;

#[tokio::test]
async fn ci_failure_records_succeeded_ci_failed_outcome() {
    let (_dir, receipt, _events) = deploy_with_fake_outcome(FakeOutcome::CiFail, false).await;

    assert_eq!(receipt.outcome, DeployOutcome::SucceededCiFailed);
}

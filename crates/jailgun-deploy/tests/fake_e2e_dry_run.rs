#![cfg(feature = "fake-backends")]

mod support;

use jailgun_deploy::{fake::FakeOutcome, DeployOutcome};
use support::fake_e2e::deploy_with_fake_outcome;

#[tokio::test]
async fn dry_run_outcome_is_dry_run_staged() {
    let (_dir, receipt, _events) = deploy_with_fake_outcome(FakeOutcome::Success, true).await;

    assert_eq!(receipt.outcome, DeployOutcome::DryRunStaged);
}

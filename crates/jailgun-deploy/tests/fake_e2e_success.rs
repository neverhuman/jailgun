#![cfg(feature = "fake-backends")]

mod support;

use jailgun_deploy::{fake::FakeOutcome, DeployOutcome};
use support::fake_e2e::deploy_with_fake_outcome;

#[tokio::test]
async fn success_path_writes_receipt_and_emits_deploy_finished() {
    let (_dir, receipt, events) = deploy_with_fake_outcome(FakeOutcome::Success, false).await;

    assert_eq!(receipt.outcome, DeployOutcome::Succeeded);
    assert!(receipt.receipt_path.is_some());

    let receipt_path = receipt.receipt_path.as_ref().unwrap();
    assert!(tokio::fs::metadata(receipt_path).await.is_ok());
    assert!(!events.is_empty());
}

mod support;

use jailgun_core::CleanupPolicy;
use jailgun_deploy::{cleanup_remote_checkout, CleanupOutcome, RemoteSnapshot};
use support::{cleanup_request, FakeCleanupRemote};

#[tokio::test]
async fn preserve_reset_creates_ref_writes_receipt_and_resets() {
    let mut remote = FakeCleanupRemote::new(vec![
        RemoteSnapshot::clean("head-a", "origin-b"),
        RemoteSnapshot::clean("head-a", "origin-c"),
        RemoteSnapshot::clean("origin-c", "origin-c"),
    ]);
    let receipt =
        cleanup_remote_checkout(&mut remote, cleanup_request(CleanupPolicy::PreserveReset))
            .await
            .expect("preserve reset");
    assert_eq!(receipt.outcome, CleanupOutcome::PreservedReset);
    assert_eq!(receipt.preserved_sha.as_deref(), Some("head-a"));
    assert_eq!(remote.refs.len(), 1);
    assert!(remote.refs[0]
        .0
        .starts_with("refs/heads/jailgun-preserved/run-one-"));
    assert_eq!(remote.reset_targets, vec!["origin-c"]);
    assert_eq!(remote.receipt_writes, 2);
    assert!(receipt
        .receipt_path
        .as_ref()
        .expect("receipt path")
        .starts_with("receipts/run-one"));
}

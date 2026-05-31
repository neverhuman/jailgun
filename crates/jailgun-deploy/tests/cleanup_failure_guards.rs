mod support;

use jailgun_core::CleanupPolicy;
use jailgun_deploy::{cleanup_remote_checkout, CleanupError, RemoteSnapshot};
use support::{cleanup_request, FakeCleanupRemote};

#[tokio::test]
async fn dirty_remote_stops_before_preserve() {
    let mut remote = FakeCleanupRemote::new(vec![RemoteSnapshot::dirty(
        "head-a",
        "origin-b",
        " M src/lib.rs",
    )]);
    let error = cleanup_remote_checkout(&mut remote, cleanup_request(CleanupPolicy::PreserveReset))
        .await
        .expect_err("dirty");
    assert!(matches!(error, CleanupError::DirtyRemote { .. }));
    assert!(remote.refs.is_empty());
}

#[tokio::test]
async fn receipt_failure_stops_before_reset() {
    let mut remote = FakeCleanupRemote::new(vec![RemoteSnapshot::clean("head-a", "origin-b")]);
    remote.fail_receipt = true;
    let error = cleanup_remote_checkout(&mut remote, cleanup_request(CleanupPolicy::PreserveReset))
        .await
        .expect_err("receipt failed");
    assert!(matches!(error, CleanupError::Receipt(_)));
    assert_eq!(remote.refs.len(), 1);
    assert!(remote.reset_targets.is_empty());
}

#[tokio::test]
async fn ref_failure_stops_before_receipt_and_reset() {
    let mut remote = FakeCleanupRemote::new(vec![RemoteSnapshot::clean("head-a", "origin-b")]);
    remote.fail_ref = true;
    let error = cleanup_remote_checkout(&mut remote, cleanup_request(CleanupPolicy::PreserveReset))
        .await
        .expect_err("ref failed");
    assert!(matches!(error, CleanupError::PreserveRef(_)));
    assert_eq!(remote.receipt_writes, 0);
    assert!(remote.reset_targets.is_empty());
}

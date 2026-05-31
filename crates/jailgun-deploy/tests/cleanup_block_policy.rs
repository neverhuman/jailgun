mod support;

use jailgun_core::CleanupPolicy;
use jailgun_deploy::{cleanup_remote_checkout, CleanupError, RemoteSnapshot};
use support::{cleanup_request, FakeCleanupRemote};

#[tokio::test]
async fn block_policy_stops_clean_divergence() {
    let mut remote = FakeCleanupRemote::new(vec![RemoteSnapshot::clean("head-a", "origin-b")]);
    let error = cleanup_remote_checkout(&mut remote, cleanup_request(CleanupPolicy::Block))
        .await
        .expect_err("blocked");
    assert!(matches!(error, CleanupError::DivergentBlocked { .. }));
    assert!(remote.refs.is_empty());
}

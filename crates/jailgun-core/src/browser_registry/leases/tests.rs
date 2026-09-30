use super::*;
use crate::BrowserAccountRoots;

#[test]
fn migration_waits_for_archive_owners_then_permanently_routes_profiles_to_workflow() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("accounts.json");
    ready_registry(&path, &[("account", 10)]);
    let manager = BrowserLeaseManager::new(&path);
    let mut lease = manager.try_acquire(&request("active", 1)).unwrap();
    assert!(manager.claim_workflow_profiles(temp.path()).is_err());
    lease.release().unwrap();
    manager.claim_workflow_profiles(temp.path()).unwrap();
    manager.claim_workflow_profiles(temp.path()).unwrap();
    assert!(manager
        .try_acquire(&request("blocked", 1))
        .unwrap_err()
        .to_string()
        .contains("workflow-runtime-owned"));
    assert!(manager
        .claim_workflow_profiles(&temp.path().join("different-runtime"))
        .is_err());
}

fn ready_registry(path: &Path, accounts: &[(&str, u16)]) {
    let roots = BrowserAccountRoots {
        profile_root: path.parent().unwrap().join("profiles"),
        state_root: path.parent().unwrap().join("state"),
        downloads_root: path.parent().unwrap().join("downloads"),
    };
    let mut registry = BrowserProfileRegistry::default();
    for (index, (id, max_tabs)) in accounts.iter().enumerate() {
        registry
            .upsert_account(
                &format!("{id}@example.invalid"),
                Some((*id).to_string()),
                &roots,
                9224 + index as u16,
                *max_tabs,
            )
            .expect("upsert account");
        registry.account_mut(id).unwrap().status = BrowserAccountStatus::Ready;
    }
    registry.save(path).expect("save registry");
}

fn request(run_id: &str, tabs: u16) -> BrowserLeaseRequest {
    BrowserLeaseRequest {
        run_id: run_id.to_string(),
        account_ids: Vec::new(),
        tabs,
        allow_queueing: false,
        queue_timeout_seconds: 1,
        lease_ttl_seconds: 60,
    }
}

#[test]
fn leases_share_capacity_across_managers() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry_path = temp.path().join("browser-profiles.json");
    ready_registry(&registry_path, &[("acct-a", 1)]);
    let first = BrowserLeaseManager::new(&registry_path);
    let second = BrowserLeaseManager::new(&registry_path);

    let mut lease = first.try_acquire(&request("run-1", 1)).expect("lease");
    let busy = second
        .try_acquire(&request("run-2", 1))
        .expect_err("capacity is busy");
    assert!(matches!(
        busy,
        BrowserRegistryError::LeaseUnavailable { .. }
    ));

    lease.release().expect("release");
    let next = second
        .try_acquire(&request("run-2", 1))
        .expect("capacity released");
    assert_eq!(next.accounts().len(), 1);
}

#[test]
fn allocates_to_least_loaded_ready_accounts() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry_path = temp.path().join("browser-profiles.json");
    ready_registry(&registry_path, &[("acct-a", 2), ("acct-b", 2)]);
    let manager = BrowserLeaseManager::new(&registry_path);

    let first = manager.try_acquire(&request("run-1", 1)).expect("first");
    assert_eq!(first.allocations()[0].account_id, "acct-a");

    let second = manager.try_acquire(&request("run-2", 1)).expect("second");
    assert_eq!(second.allocations()[0].account_id, "acct-b");
}

#[test]
fn explicit_accounts_reject_unknown_and_non_ready() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry_path = temp.path().join("browser-profiles.json");
    ready_registry(&registry_path, &[("acct-a", 1)]);
    let mut registry = BrowserProfileRegistry::load_or_default(&registry_path).unwrap();
    let roots = BrowserAccountRoots {
        profile_root: temp.path().join("profiles"),
        state_root: temp.path().join("state"),
        downloads_root: temp.path().join("downloads"),
    };
    registry
        .upsert_account("b@example.invalid", Some("acct-b".into()), &roots, 9226, 1)
        .unwrap();
    registry.save(&registry_path).unwrap();
    let manager = BrowserLeaseManager::new(&registry_path);

    let mut missing = request("run-missing", 1);
    missing.account_ids = vec!["acct-missing".into()];
    assert!(matches!(
        manager.try_acquire(&missing),
        Err(BrowserRegistryError::MissingAccount(_))
    ));

    let mut not_ready = request("run-auth", 1);
    not_ready.account_ids = vec!["acct-b".into()];
    assert!(matches!(
        manager.try_acquire(&not_ready),
        Err(BrowserRegistryError::AccountNotReady { .. })
    ));
}

#[test]
fn stale_leases_are_purged_by_expiry() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry_path = temp.path().join("browser-profiles.json");
    ready_registry(&registry_path, &[("acct-a", 1)]);
    let manager = BrowserLeaseManager::new(&registry_path);
    let now = unix_seconds();
    save_lease_state(
        manager.lease_path(),
        &BrowserLeaseState {
            version: 1,
            leases: vec![BrowserLeaseRecord {
                lease_id: "stale".into(),
                run_id: "old-run".into(),
                account_id: "acct-a".into(),
                tabs: 1,
                owner_pid: std::process::id(),
                started_at_epoch_secs: now.saturating_sub(120),
                expires_at_epoch_secs: now.saturating_sub(1),
            }],
        },
    )
    .expect("seed stale lease");

    let lease = manager.try_acquire(&request("run-new", 1)).expect("lease");
    assert_eq!(lease.accounts().len(), 1);
}

#[test]
fn same_process_same_second_release_preserves_other_run() {
    let temp = tempfile::tempdir().unwrap();
    let registry_path = temp.path().join("browser-profiles.json");
    ready_registry(&registry_path, &[("acct-a", 2)]);
    let manager = BrowserLeaseManager::new(&registry_path);
    let now = unix_seconds();
    let mut first = manager
        .try_acquire_at(&request("run-first", 1), now)
        .unwrap();
    let second = manager
        .try_acquire_at(&request("run-second", 1), now)
        .unwrap();
    first.release().unwrap();
    first.release().unwrap();
    let remaining = load_lease_state(manager.lease_path()).unwrap();
    assert_eq!(
        remaining.leases.len(),
        1,
        "release removed another run's reservation"
    );
    assert_eq!(remaining.leases[0].run_id, "run-second");
    assert_eq!(second.allocations()[0].tabs, 1);
    assert!(matches!(
        manager.try_acquire(&request("run-third", 2)),
        Err(BrowserRegistryError::LeaseUnavailable { .. })
    ));
}

#[cfg(unix)]
#[test]
fn migration_rejects_linked_ownership() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/profile-owner-proofs");
    fs::create_dir_all(&parent).unwrap();
    let temp = tempfile::tempdir_in(parent).unwrap();
    let path = temp.path().join("accounts.json");
    ready_registry(&path, &[("account", 10)]);
    let registry = BrowserProfileRegistry::load_or_default(&path).unwrap();
    let profile = &registry.accounts[0].profile_dir;
    let target = temp.path().join("foreign-owner-file");
    let contents = format!("{}\n", temp.path().display());
    fs::write(&target, &contents).unwrap();
    let marker = profile.join(".jailgun-workflow-owner");
    std::os::unix::fs::symlink(&target, &marker).unwrap();
    let manager = BrowserLeaseManager::new(&path);
    assert!(
        manager.claim_workflow_profiles(temp.path()).is_err(),
        "migration accepted a linked ownership marker"
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), contents);
    assert!(fs::symlink_metadata(&marker).unwrap().is_symlink());
}

#[cfg(unix)]
#[test]
fn archive_rejects_dangling_profile_owner_marker() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/profile-owner-proofs");
    fs::create_dir_all(&parent).unwrap();
    let temp = tempfile::tempdir_in(parent).unwrap();
    let path = temp.path().join("accounts.json");
    ready_registry(&path, &[("account", 10)]);
    let registry = BrowserProfileRegistry::load_or_default(&path).unwrap();
    let marker = registry.accounts[0]
        .profile_dir
        .join(".jailgun-workflow-owner");
    std::os::unix::fs::symlink(temp.path().join("absent"), &marker).unwrap();
    assert!(
        BrowserLeaseManager::new(&path)
            .try_acquire(&request("blocked", 1))
            .is_err(),
        "archive adopted a profile with a dangling owner marker"
    );
    assert!(fs::symlink_metadata(marker).unwrap().is_symlink());
}

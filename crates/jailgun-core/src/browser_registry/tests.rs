use super::*;

#[test]
fn derives_stable_account_id_from_email_hint() {
    assert_eq!(
        default_account_id("USER@gmail.com"),
        default_account_id("user@gmail.com")
    );
    assert!(default_account_id("user@gmail.com").starts_with("acct-"));
    assert_eq!(default_account_id("user@gmail.com").len(), 13);
}

#[test]
fn rejects_unsafe_account_ids() {
    for id in ["../acct", "acct/a", "acct=a", "acct:a", ".."] {
        assert!(validate_account_id(id).is_err(), "{id} should be rejected");
    }
    validate_account_id("acct-safe_1.2").expect("safe id");
}

#[test]
fn upsert_creates_private_runtime_dirs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let roots = BrowserAccountRoots {
        profile_root: temp.path().join("profiles"),
        state_root: temp.path().join("state"),
        downloads_root: temp.path().join("downloads"),
    };
    let mut registry = BrowserProfileRegistry::default();
    let account = registry
        .upsert_account("user@example.com", None, &roots, 9224, 3)
        .expect("upsert account");
    assert!(account.profile_dir.is_dir());
    assert!(account.state_dir.is_dir());
    assert!(account.downloads_dir.is_dir());
    assert_eq!(registry.accounts.len(), 1);
}

#[test]
fn repeated_setup_preserves_account_binding_paths_port_and_capacity() {
    let temp = tempfile::tempdir().unwrap();
    let roots = BrowserAccountRoots {
        profile_root: temp.path().join("profiles"),
        state_root: temp.path().join("state"),
        downloads_root: temp.path().join("downloads"),
    };
    let mut registry = BrowserProfileRegistry::default();
    registry
        .upsert_account(
            "user@example.invalid",
            Some("acct-a".into()),
            &roots,
            9224,
            3,
        )
        .unwrap();
    registry.account_mut("acct-a").unwrap().status = BrowserAccountStatus::Ready;
    let original = registry.require_account("acct-a").unwrap().clone();
    let other_roots = BrowserAccountRoots {
        profile_root: temp.path().join("other-profiles"),
        ..roots.clone()
    };
    let again = registry
        .upsert_account(
            "USER@example.invalid",
            Some("acct-a".into()),
            &other_roots,
            9230,
            10,
        )
        .unwrap();
    assert_eq!(again, original);
    assert_eq!(registry.require_account("acct-a").unwrap(), &original);
    assert!(!other_roots.profile_root.exists());
    assert!(registry
        .upsert_account(
            "other@example.invalid",
            Some("acct-a".into()),
            &roots,
            9224,
            3
        )
        .is_err());
    assert!(registry
        .upsert_account(
            "other@example.invalid",
            Some("acct-b".into()),
            &roots,
            9224,
            3
        )
        .is_err());
    assert_eq!(registry.accounts.len(), 1);
}

#[cfg(unix)]
#[test]
fn every_new_runtime_directory_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("private-parent");
    let leaf = parent.join("nested/profile");
    ensure_private_dir(&leaf).unwrap();
    for path in [&parent, &parent.join("nested"), &leaf] {
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
}

#[test]
fn concurrent_registration_preserves_all_accounts_and_skips_occupied_ports() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("browser-profiles.json");
    let roots = BrowserAccountRoots {
        profile_root: temp.path().join("profiles"),
        state_root: temp.path().join("state"),
        downloads_root: temp.path().join("downloads"),
    };
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = occupied.local_addr().unwrap().port();
    if port > 65530 {
        return;
    }
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    let workers: Vec<_> = (0..4)
        .map(|n| {
            let (path, roots, barrier) = (path.clone(), roots.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                BrowserProfileRegistry::register_account(
                    &path,
                    &format!("user{n}@example.invalid"),
                    Some(format!("acct-{n}")),
                    &roots,
                    port,
                    10,
                )
                .unwrap()
            })
        })
        .collect();
    let accounts: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    let ports: std::collections::BTreeSet<_> = accounts.iter().map(|a| a.cdp_port).collect();
    assert_eq!(ports.len(), 4);
    assert!(!ports.contains(&port));
    assert_eq!(
        BrowserProfileRegistry::load_or_default(&path)
            .unwrap()
            .accounts
            .len(),
        4
    );
}

#[test]
fn verified_identity_cannot_replace_an_existing_binding() {
    let temp = tempfile::tempdir().unwrap();
    let roots = BrowserAccountRoots {
        profile_root: temp.path().join("profiles"),
        state_root: temp.path().join("state"),
        downloads_root: temp.path().join("downloads"),
    };
    let mut registry = BrowserProfileRegistry::default();
    let mut account = registry
        .upsert_account("synthetic@example.invalid", None, &roots, 9224, 10)
        .unwrap();
    let first = ProviderIdentity {
        id: "user-one".into(),
        email: "synthetic@example.invalid".into(),
    };
    account
        .confirm_identity(&first, "verified-once".into())
        .unwrap();
    let second = ProviderIdentity {
        id: "user-two".into(),
        ..first
    };
    assert!(account.confirm_identity(&second, "invalid".into()).is_err());
    assert_eq!(account.provider_account_id.as_deref(), Some("user-one"));
    assert_eq!(account.last_verified_at.as_deref(), Some("verified-once"));
}

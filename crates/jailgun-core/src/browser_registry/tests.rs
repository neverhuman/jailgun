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

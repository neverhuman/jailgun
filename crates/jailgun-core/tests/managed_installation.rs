#![cfg(unix)]
use jailgun_core::managed_installation::ManagedInstallation;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn fixture() -> (tempfile::TempDir, String) {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/installation-use-proofs");
    fs::create_dir_all(&parent).unwrap();
    let directory = tempfile::tempdir_in(parent.canonicalize().unwrap()).unwrap();
    let prefix = directory.path().canonicalize().unwrap();
    let name = format!("0.2.0-linux-x64-{}", "a".repeat(64));
    let base = prefix.join("lib/jailgun");
    fs::create_dir_all(prefix.join("bin")).unwrap();
    fs::create_dir_all(base.join("releases").join(&name).join("bin")).unwrap();
    for binary in ["jailgun", "jailhard"] {
        fs::write(
            base.join("releases").join(&name).join("bin").join(binary),
            b"synthetic executable",
        )
        .unwrap();
        std::os::unix::fs::symlink(
            format!("../lib/jailgun/current/bin/{binary}"),
            prefix.join("bin").join(binary),
        )
        .unwrap();
    }
    std::os::unix::fs::symlink(format!("releases/{name}"), base.join("current")).unwrap();
    fs::write(base.join(".usage.lock"), b"").unwrap();
    let manifest = serde_json::to_vec(&serde_json::json!({"schema_version":1,"files":[
        {"path":"bin/jailgun","bytes":20,"executable":false,"sha256":format!("{:x}",Sha256::digest(b"synthetic executable"))},
        {"path":"bin/jailhard","bytes":20,"executable":false,"sha256":format!("{:x}",Sha256::digest(b"synthetic executable"))}
    ]})).unwrap();
    fs::write(
        base.join("releases").join(&name).join("manifest.json"),
        &manifest,
    )
    .unwrap();
    fs::write(base.join("installation.json"), serde_json::to_vec(&serde_json::json!({
        "schema_version":1,"application":"jailgun","prefix":prefix,
        "releases":{name.clone():{"archive_sha256":"a".repeat(64),"manifest_sha256":format!("{:x}",Sha256::digest(&manifest)),"usage_lock_version":1}}
    })).unwrap()).unwrap();
    (directory, name)
}

#[test]
fn uninstall_removes_only_verified_application_files_and_preserves_runtime_and_other_tools() {
    let (directory, _) = fixture();
    let prefix = directory.path();
    fs::create_dir(prefix.join(".jailgun")).unwrap();
    fs::write(prefix.join(".jailgun/results"), "retained synthetic result").unwrap();
    fs::write(prefix.join("bin/another-tool"), "another tool").unwrap();
    jailgun_core::managed_installation::uninstall(prefix).unwrap();
    assert!(!prefix.join("lib/jailgun").exists());
    assert!(prefix.join("bin/jailgun").symlink_metadata().is_err());
    assert_eq!(
        fs::read_to_string(prefix.join(".jailgun/results")).unwrap(),
        "retained synthetic result"
    );
    assert_eq!(
        fs::read_to_string(prefix.join("bin/another-tool")).unwrap(),
        "another tool"
    );
}

#[test]
fn removal_refuses_busy_corrupt_linked_and_unregistered_files_before_deleting_anything() {
    for kind in [
        "busy",
        "installer",
        "corrupt",
        "linked",
        "extra-file",
        "extra-directory",
        "manifest",
        "current",
    ] {
        let (directory, name) = fixture();
        let base = directory.path().join("lib/jailgun");
        let release = base.join("releases").join(name);
        let installation = ManagedInstallation::load(directory.path()).unwrap();
        let _guard = (kind == "busy").then(|| installation.shared_use().unwrap());
        match kind {
            "installer" => fs::create_dir(base.join(".install-lock")).unwrap(),
            "corrupt" => fs::write(release.join("bin/jailhard"), "modified executable").unwrap(),
            "linked" => {
                fs::remove_file(release.join("bin/jailhard")).unwrap();
                std::os::unix::fs::symlink("jailgun", release.join("bin/jailhard")).unwrap();
            }
            "extra-file" => fs::write(release.join("foreign"), "retained").unwrap(),
            "extra-directory" => fs::create_dir(release.join("foreign")).unwrap(),
            "manifest" => fs::write(release.join("manifest.json"), "{}").unwrap(),
            "current" => {
                fs::remove_file(base.join("current")).unwrap();
                std::os::unix::fs::symlink("../../foreign", base.join("current")).unwrap();
            }
            _ => {}
        }
        assert!(
            jailgun_core::managed_installation::uninstall(directory.path()).is_err(),
            "{kind}"
        );
        assert_eq!(
            fs::read(release.join("bin/jailgun")).unwrap(),
            b"synthetic executable",
            "{kind}"
        );
        assert!(!base.join(".uninstall.json").exists(), "{kind}");
    }
}

#[test]
fn removal_journal_resumes_missing_owned_files_but_rejects_changes_after_preparation() {
    use jailgun_core::managed_installation::{uninstall, Removal};
    let (directory, name) = fixture();
    let base = directory.path().join("lib/jailgun");
    let release = base.join("releases").join(name);
    let removal = Removal::prepare(directory.path()).unwrap();
    fs::write(release.join("new-file"), "retain this").unwrap();
    assert!(removal.execute().is_err());
    assert_eq!(
        fs::read(release.join("bin/jailgun")).unwrap(),
        b"synthetic executable"
    );
    fs::remove_file(release.join("new-file")).unwrap();
    // Simulate a stopped removal after one verified file and the public link were removed.
    fs::remove_file(release.join("bin/jailgun")).unwrap();
    fs::remove_file(directory.path().join("bin/jailgun")).unwrap();
    assert!(ManagedInstallation::load(directory.path())
        .unwrap()
        .shared_use()
        .is_err());
    uninstall(directory.path()).unwrap();
    assert!(!base.exists());
}

#[test]
fn application_use_guards_cover_every_process_and_release_only_their_own_lock() {
    let (directory, _) = fixture();
    let installation = ManagedInstallation::load(directory.path()).unwrap();
    let first = installation.shared_use().unwrap();
    let second = installation.shared_use().unwrap();
    assert!(installation.exclusive_use().unwrap_err().code() == "installation-busy");
    drop(first);
    assert!(installation.exclusive_use().is_err());
    drop(second);
    let removal = installation.exclusive_use().unwrap();
    assert!(installation.shared_use().is_err());
    drop(removal);
    installation.shared_use().unwrap();
}

#[test]
fn discovery_preserves_manual_bundles_and_resolves_only_registered_active_links() {
    let (directory, _) = fixture();
    let installation = ManagedInstallation::for_executable(&directory.path().join("bin/jailgun"))
        .unwrap()
        .unwrap();
    assert_eq!(
        installation.active_binary("jailgun").unwrap(),
        directory.path().join("bin/jailgun")
    );
    let manual = directory.path().join("manual/bin/jailgun");
    fs::create_dir_all(manual.parent().unwrap()).unwrap();
    fs::write(&manual, "synthetic executable").unwrap();
    assert!(ManagedInstallation::for_executable(&manual)
        .unwrap()
        .is_none());
    fs::remove_file(directory.path().join("bin/jailgun")).unwrap();
    std::os::unix::fs::symlink(&manual, directory.path().join("bin/jailgun")).unwrap();
    assert!(installation.active_binary("jailgun").is_err());
    assert_eq!(fs::read_to_string(manual).unwrap(), "synthetic executable");
}

#[test]
fn linked_usage_files_and_foreign_inventory_cannot_authorize_removal() {
    let (directory, name) = fixture();
    let base = directory.path().join("lib/jailgun");
    let foreign = directory.path().join("foreign");
    fs::write(&foreign, "retained content").unwrap();
    fs::remove_file(base.join(".usage.lock")).unwrap();
    std::os::unix::fs::symlink(&foreign, base.join(".usage.lock")).unwrap();
    let installation = ManagedInstallation::load(directory.path()).unwrap();
    assert!(installation.shared_use().is_err());
    assert!(installation.exclusive_use().is_err());
    assert_eq!(fs::read_to_string(&foreign).unwrap(), "retained content");
    let marker = base.join("installation.json");
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(&marker).unwrap()).unwrap();
    metadata["prefix"] = serde_json::json!(directory.path().join("another prefix"));
    fs::write(&marker, serde_json::to_vec(&metadata).unwrap()).unwrap();
    assert!(ManagedInstallation::load(directory.path()).is_err());
    metadata["prefix"] = serde_json::json!(directory.path());
    let record = metadata["releases"]
        .as_object_mut()
        .unwrap()
        .remove(&name)
        .unwrap();
    metadata["releases"]["../foreign"] = record;
    fs::write(marker, serde_json::to_vec(&metadata).unwrap()).unwrap();
    assert!(ManagedInstallation::load(directory.path()).is_err());
}

#[test]
fn older_releases_can_remain_installed_but_cannot_be_assumed_to_hold_use_locks() {
    let (directory, name) = fixture();
    let marker = directory.path().join("lib/jailgun/installation.json");
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(&marker).unwrap()).unwrap();
    metadata["releases"][&name]
        .as_object_mut()
        .unwrap()
        .remove("usage_lock_version");
    fs::write(marker, serde_json::to_vec(&metadata).unwrap()).unwrap();
    let installation = ManagedInstallation::load(directory.path()).unwrap();
    installation.shared_use().unwrap();
    assert!(installation.exclusive_use().unwrap_err().code() == "installation-lock-unsupported");
}

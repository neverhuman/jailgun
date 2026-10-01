#![cfg(unix)]
use jailgun_core::{managed_installation::ManagedInstallation, sha256_file};
use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};

fn installed(root: &Path) -> String {
    let name = format!("0.2.0-linux-x64-{}", "a".repeat(64));
    let base = root.join("lib/jailgun");
    let release = base.join("releases").join(&name);
    fs::create_dir_all(release.join("bin")).unwrap();
    fs::create_dir(root.join("bin")).unwrap();
    let mut files = Vec::new();
    for (name, executable) in [
        ("jailgun", env!("CARGO_BIN_EXE_jailgun")),
        ("jailhard", env!("CARGO_BIN_EXE_jailhard")),
    ] {
        let path = release.join("bin").join(name);
        fs::hard_link(executable, &path).unwrap();
        files.push(json!({"path":format!("bin/{name}"),"bytes":fs::metadata(&path).unwrap().len(),"sha256":sha256_file(&path).unwrap(),"executable":true}));
        std::os::unix::fs::symlink(
            format!("../lib/jailgun/current/bin/{name}"),
            root.join("bin").join(name),
        )
        .unwrap();
    }
    let manifest = release.join("manifest.json");
    fs::write(
        &manifest,
        serde_json::to_vec(&json!({"schema_version":1,"files":files})).unwrap(),
    )
    .unwrap();
    fs::write(base.join("installation.json"), serde_json::to_vec(&json!({"schema_version":1,"application":"jailgun","prefix":root,"releases":{name.clone():{"archive_sha256":"a".repeat(64),"manifest_sha256":sha256_file(&manifest).unwrap(),"usage_lock_version":1}}})).unwrap()).unwrap();
    fs::write(base.join(".usage.lock"), "").unwrap();
    std::os::unix::fs::symlink(format!("releases/{name}"), base.join("current")).unwrap();
    name
}

#[test]
fn installed_binaries_honor_removal_locks_and_cli_preserves_private_runtime() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/cli-uninstall-proofs");
    fs::create_dir_all(&parent).unwrap();
    let root = tempfile::Builder::new()
        .prefix("install with spaces ")
        .tempdir_in(parent.canonicalize().unwrap())
        .unwrap();
    installed(root.path());
    let installation = ManagedInstallation::load(root.path()).unwrap();
    let exclusive = installation.exclusive_use().unwrap();
    for (name, args) in [
        ("jailgun", vec!["fixture", "runs", "--json"]),
        ("jailhard", vec![]),
    ] {
        let output = Command::new(root.path().join("bin").join(name))
            .args(args)
            .env_clear()
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("installation-busy"));
    }
    drop(exclusive);
    let shared = installation.shared_use().unwrap();
    let binary = root.path().join("bin/jailgun");
    let busy = Command::new(&binary)
        .args(["uninstall", "--json"])
        .env_clear()
        .output()
        .unwrap();
    assert!(!busy.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&busy.stderr).unwrap()["code"],
        "installation-busy"
    );
    drop(shared);
    fs::create_dir(root.path().join(".jailgun")).unwrap();
    fs::write(
        root.path().join(".jailgun/result.md"),
        "synthetic retained result",
    )
    .unwrap();
    let output = Command::new(binary)
        .args(["uninstall", "--json"])
        .env_clear()
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "completed");
    assert_eq!(report["runtime_data"], "unchanged");
    assert!(!root.path().join("lib/jailgun").exists());
    assert_eq!(
        fs::read_to_string(root.path().join(".jailgun/result.md")).unwrap(),
        "synthetic retained result"
    );
}

use super::guarded_reset_script;
use std::{fs, path::Path, process::Command};

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

#[test]
fn guarded_reset_checks_current_checkout_and_preservation_under_mutation_lock() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy-shell-proofs");
    fs::create_dir_all(&root).unwrap();
    for scenario in ["dirty", "head", "ref", "target", "owned", "valid"] {
        let dir = tempfile::Builder::new()
            .prefix(scenario)
            .tempdir_in(&root)
            .unwrap();
        let repo = dir.path();
        git(repo, &["init", "-b", "main"]);
        git(repo, &["config", "user.name", "Synthetic Operator"]);
        git(repo, &["config", "user.email", "synthetic@example.invalid"]);
        git(repo, &["config", "core.hooksPath", "/dev/null"]);
        fs::write(repo.join("work.txt"), "initial").unwrap();
        git(repo, &["add", "."]);
        git(repo, &["commit", "-m", "initial"]);
        let target = git(repo, &["rev-parse", "HEAD"]);
        git(repo, &["update-ref", "refs/remotes/origin/main", &target]);
        fs::write(repo.join("work.txt"), "new committed work").unwrap();
        git(repo, &["add", "."]);
        git(repo, &["commit", "-m", "new work"]);
        let head = git(repo, &["rev-parse", "HEAD"]);
        let preserved = "refs/heads/jailgun-preserved/test";
        git(repo, &["update-ref", preserved, &head]);
        match scenario {
            "dirty" => fs::write(repo.join("operator.txt"), "untracked work").unwrap(),
            "ref" => {
                git(repo, &["update-ref", preserved, &target]);
            }
            "target" => {
                git(repo, &["update-ref", "refs/remotes/origin/main", &head]);
            }
            "owned" => fs::create_dir(repo.join(".git/jailgun-mutation.lock")).unwrap(),
            _ => (),
        }
        let script = guarded_reset_script(
            if scenario == "head" { &target } else { &head },
            preserved,
            &target,
        );
        let output = Command::new("bash")
            .arg("-c")
            .arg(script)
            .current_dir(repo)
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            scenario == "valid",
            "{scenario}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            git(repo, &["rev-parse", "HEAD"]),
            if scenario == "valid" {
                target.as_str()
            } else {
                head.as_str()
            },
            "{scenario}"
        );
        assert_eq!(
            repo.join(".git/jailgun-mutation.lock").exists(),
            scenario == "owned"
        );
        if scenario == "dirty" {
            assert_eq!(
                fs::read_to_string(repo.join("operator.txt")).unwrap(),
                "untracked work"
            );
        }
        if scenario == "valid" {
            assert_eq!(git(repo, &["rev-parse", preserved]), head);
        }
    }
}

#[tokio::test]
async fn failed_receipt_replacement_keeps_the_durable_previous_receipt() {
    use crate::cleanup::{CleanupReceipt, RemoteGitBackend};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy-shell-proofs");
    fs::create_dir_all(&root).unwrap();
    let dir = tempfile::Builder::new()
        .prefix("receipt-")
        .tempdir_in(&root)
        .unwrap();
    let path = dir.path().join("receipt.json");
    fs::write(&path, "previous receipt").unwrap();
    let staging = path.with_extension("json.writing");
    fs::write(&staging, "interrupted replacement").unwrap();
    let receipt: CleanupReceipt = serde_json::from_value(serde_json::json!({
        "run_id":"synthetic-run", "remote_host":"synthetic.invalid", "remote_dir":"synthetic",
        "policy":"preserve-reset", "outcome":"preserved-reset", "timestamp":"synthetic",
        "final_status_short":"", "receipt_path":path,
    }))
    .unwrap();
    let mut backend = super::SshRemoteGit::new("synthetic.invalid", dir.path());
    assert!(backend.write_receipt(&receipt).await.is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "previous receipt");
    fs::remove_file(staging).unwrap();
    backend.write_receipt(&receipt).await.unwrap();
    let written: CleanupReceipt = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(written, receipt);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

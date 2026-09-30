use jailgun_deploy::{build_launcher_script, JobSpec};
use std::{fs, path::Path, process::Command};

fn git(repo: &Path, args: &[&str]) -> String {
    let result = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().into()
}

fn proof(command: &str, stash: bool) -> (tempfile::TempDir, String, std::process::Output) {
    proof_with_ownership(command, stash, false)
}

fn proof_with_ownership(
    command: &str,
    stash: bool,
    owned: bool,
) -> (tempfile::TempDir, String, std::process::Output) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy-shell-proofs");
    fs::create_dir_all(&root).unwrap();
    let dir = tempfile::Builder::new()
        .prefix("preservation-")
        .tempdir_in(root)
        .unwrap();
    let repo = dir.path().join("checkout with spaces");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-b", "main"]);
    git(&repo, &["config", "user.name", "Synthetic Operator"]);
    git(
        &repo,
        &["config", "user.email", "synthetic@example.invalid"],
    );
    git(&repo, &["config", "core.hooksPath", "/dev/null"]);
    fs::write(repo.join("tracked.txt"), "initial\n").unwrap();
    git(&repo, &["add", "tracked.txt"]);
    git(&repo, &["commit", "-m", "synthetic initial state"]);
    let head = git(&repo, &["rev-parse", "HEAD"]);
    if owned {
        fs::create_dir(repo.join(".git/jailgun-mutation.lock")).unwrap();
    }
    let input = dir.path().join("input");
    fs::create_dir(&input).unwrap();
    fs::write(input.join("tracked.txt"), "new generated work\n").unwrap();
    let upload = dir.path().join("job/uploads");
    fs::create_dir_all(&upload).unwrap();
    let archive = upload.join("synthetic.tar.gz");
    assert!(Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(&input)
        .arg("tracked.txt")
        .status()
        .unwrap()
        .success());
    let script = build_launcher_script(&JobSpec {
        run_id: "synthetic-preservation".into(),
        tab_id: 1,
        remote_dir: repo.to_string_lossy().into(),
        remote_archive_path: archive.to_string_lossy().into(),
        remote_command: command.into(),
        strip_components: 0,
        local_sha256: "a".repeat(64),
        remote_sha256: "a".repeat(64),
        stash_on_failure: stash,
    });
    let path = dir.path().join("launcher.sh");
    fs::write(&path, script).unwrap();
    let output = Command::new("bash").arg(path).output().unwrap();
    (dir, head, output)
}

#[test]
fn existing_mutation_owner_prevents_extraction_and_is_not_removed() {
    let (dir, initial, output) = proof_with_ownership("true", true, true);
    let repo = dir.path().join("checkout with spaces");
    assert!(!output.status.success());
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), initial);
    assert_eq!(
        fs::read_to_string(repo.join("tracked.txt")).unwrap(),
        "initial\n"
    );
    assert!(repo.join(".git/jailgun-mutation.lock").is_dir());
    let status: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("job/status.json")).unwrap()).unwrap();
    assert_eq!(status["failure_reason"], "checkout-owned");
}

#[test]
fn failed_preservation_ref_never_resets_new_commits() {
    let (dir, initial, output) = proof("git add tracked.txt; git commit -m synthetic-new-work; mkdir -p .git/refs/heads/jailgun-failed; chmod 500 .git/refs/heads/jailgun-failed; exit 42", true);
    let repo = dir.path().join("checkout with spaces");
    // Restore directory permissions so the disposable synthetic fixture can be removed.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            repo.join(".git/refs/heads/jailgun-failed"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
    }
    assert!(
        !output.status.success(),
        "failed preservation was reported as success"
    );
    assert_ne!(
        git(&repo, &["rev-parse", "HEAD"]),
        initial,
        "unpreserved commit was reset"
    );
    assert_eq!(
        fs::read_to_string(repo.join("tracked.txt")).unwrap(),
        "new generated work\n"
    );
}

#[test]
fn disabled_stashing_never_discards_dirty_work() {
    let (dir, initial, output) = proof("exit 42", false);
    let repo = dir.path().join("checkout with spaces");
    assert!(!output.status.success());
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), initial);
    assert_eq!(
        fs::read_to_string(repo.join("tracked.txt")).unwrap(),
        "new generated work\n",
        "dirty work was discarded without a stash"
    );
}

#[test]
fn receipt_write_failure_stops_before_reset() {
    let (dir, initial, output) = proof(
        "git add tracked.txt; git commit -m synthetic-new-work; chmod 500 ../job; exit 42",
        true,
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir.path().join("job"), fs::Permissions::from_mode(0o700)).unwrap();
    }
    let repo = dir.path().join("checkout with spaces");
    assert!(!output.status.success());
    assert_ne!(git(&repo, &["rev-parse", "HEAD"]), initial);
    assert_eq!(
        fs::read_to_string(repo.join("tracked.txt")).unwrap(),
        "new generated work\n"
    );
}

#[test]
fn confirmed_preservation_retains_commits_and_dirty_work_before_reset() {
    let (dir, initial, output) = proof("git add tracked.txt; git commit -m synthetic-new-work; printf 'uncommitted work' > untracked.txt; exit 42", true);
    let repo = dir.path().join("checkout with spaces");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), initial);
    assert_eq!(
        git(
            &repo,
            &[
                "show",
                "refs/heads/jailgun-failed/synthetic-preservation-tab-01:tracked.txt"
            ]
        ),
        "new generated work"
    );
    assert_eq!(
        git(
            &repo,
            &[
                "show",
                "refs/heads/jailgun-failed/synthetic-preservation-tab-01-stash^3:untracked.txt"
            ]
        ),
        "uncommitted work"
    );
    let status: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("job/status.json")).unwrap()).unwrap();
    assert_eq!(status["phase"], "failed-preserved");
}

//! Two provider identities exercise independent profiles, live refresh, cancellation and reuse.
#[test]
#[ignore = "requires pinned Node, sandboxed Chrome and the account_browser_proof example"]
fn two_browser_identities_preserve_independent_sessions_and_workflows() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("checkout root");
    let mut command = std::process::Command::new("node");
    command
        .args(["scripts/check-concept-workflows.mjs", "two-accounts"])
        .current_dir(&root)
        .stderr(std::process::Stdio::inherit())
        .env_clear();
    for key in ["PATH", "HOME", "TMPDIR", "LANG", "JAILGUN_TEST_CHROME"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let output = command
        .output()
        .expect("start bounded account-isolation proof");
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 proof output");
    print!("{stdout}");
    assert!(
        output.status.success(),
        "two-account browser proof failed: {}",
        output.status
    );
    let directory = stdout
        .lines()
        .find_map(|line| line.split_once("workflow evidence: ").map(|(_, path)| path))
        .expect("account-isolation evidence path");
    let evidence: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join(directory).join("evidence.json")).expect("proof receipt"),
    )
    .expect("valid proof receipt");
    assert_eq!(evidence["status"], "pass");
    let checks = evidence["checks"].as_array().expect("proof checks");
    for check in [
        "fresh-probe-recovers-authentication-without-login",
        "refresh-preserves-cancelled-login",
        "two-account-cookie-reuse-after-browser-restart",
    ] {
        assert!(
            checks.iter().any(|value| value == check),
            "missing browser obligation: {check}"
        );
    }
}

//! Two provider identities exercise independent profiles, expiry, cancellation and reuse.
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
        .current_dir(root)
        .env_clear();
    for key in ["PATH", "HOME", "TMPDIR", "LANG", "JAILGUN_TEST_CHROME"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let status = command
        .status()
        .expect("start bounded account-isolation proof");
    assert!(
        status.success(),
        "two-account browser proof failed: {status}"
    );
}

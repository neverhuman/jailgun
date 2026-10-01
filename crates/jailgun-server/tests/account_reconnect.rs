//! Real-browser regression, executed by ops/ci/e2e.sh after building the proof binary.
//! A delayed identity response overlaps reconnect and cancellation. The production
//! supervisor must preserve the new login state, refresh models even on a ready
//! account, and retain completed results and the existing browser allocation.
#[test]
#[ignore = "requires pinned Node, sandboxed Chrome and the account_browser_proof example"]
fn inflight_probes_cannot_overwrite_reconnect_or_cancellation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("checkout root");
    let mut command = std::process::Command::new("node");
    command
        .args(["scripts/check-concept-workflows.mjs", "reconnect"])
        .current_dir(root)
        .env_clear();
    for key in ["PATH", "HOME", "TMPDIR", "LANG", "JAILGUN_TEST_CHROME"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let status = command.status().expect("start bounded browser regression");
    assert!(
        status.success(),
        "account reconnect integration failed: {status}"
    );
}

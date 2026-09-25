use std::path::{Path, PathBuf};

use jailgun_core::{
    EventKind, JailgunAgentRunRequest, JailgunEvent, JAILGUN_AGENT_INTERFACE_VERSION,
};

use super::execute_summary::{
    artifacts_from_events, ci_status_from_events, deploy_status_from_events,
    receipt_paths_from_events,
};
use super::review::{cap_utf8, is_test_path, parse_name_status};

pub(super) fn write_test_config(root: &Path, registry_env: &str) -> PathBuf {
    let config_path = root.join("jailgun.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"[project]
name = "test-project"
repository = "git@example.com:org/example.git"

[browser]
chat_url = "https://chatgpt.com/"
model = "pro-extended"
tabs = 2
poll_interval_seconds = 1
completion_check_seconds = 1
submit_delay_seconds = 1
submit_jitter_seconds = 0
tar_wait_minutes = 1
profile_dir_env = "JAILGUN_CHROME_PROFILE_DIR"
state_dir_env = "JAILGUN_CHROME_STATE_DIR"
profile_registry_env = "{registry_env}"

[paths]
artifacts_dir = "artifacts"
downloads_dir_env = "JAILGUN_DOWNLOADS_DIR"

[source_archive]
enabled = false
repo_url_env = "JAILGUN_SOURCE_REPO_URL"
ref_name = "HEAD"
prefix = "source/"
archive_filename = "source.tar.gz"
delete_after_upload = true

[deploy]
enabled = false
dry_run = true
remote_host_env = "JAILGUN_REMOTE_HOST"
remote_dir_env = "JAILGUN_REMOTE_DIR"
remote_command_env = "JAILGUN_REMOTE_COMMAND"
remote_strip_components = 1
remote_cleanup_policy = "preserve-reset"
remote_status_poll_seconds = 1
remote_job_delay_seconds = 1
remote_job_jitter_seconds = 0

[prompt_policy]
deny_github_write_by_default = true
allow_write_prompts = false
allow_info_prompts = false
allowed_repositories = []
"#
        ),
    )
    .expect("write config");
    config_path
}

pub(super) fn base_request(prompt_file: PathBuf, config_path: PathBuf) -> JailgunAgentRunRequest {
    JailgunAgentRunRequest {
        version: JAILGUN_AGENT_INTERFACE_VERSION,
        run_id: Some("run-1".into()),
        prompt_ref: "jmcp://prompt/1".into(),
        prompt_file,
        config_path: Some(config_path),
        tabs: Some(2),
        max_runtime_seconds: Some(60),
        repo: Default::default(),
        source_archive: Default::default(),
        deploy: Default::default(),
        ci: Default::default(),
        browser: Default::default(),
        github: Default::default(),
    }
}

#[test]
fn review_packet_helpers_parse_name_status_and_tests() {
    let files = parse_name_status("M\tcrates/lib.rs\nR100\told.test.ts\tnew.test.ts\n");

    assert_eq!(files.len(), 2);
    assert_eq!(files[0].status, "M");
    assert_eq!(files[0].path, "crates/lib.rs");
    assert_eq!(files[1].status, "R100");
    assert_eq!(files[1].old_path.as_deref(), Some("old.test.ts"));
    assert_eq!(files[1].path, "new.test.ts");
    assert!(is_test_path(&files[1].path));
    assert!(!is_test_path(&files[0].path));
}

#[test]
fn caps_review_patch_on_utf8_boundary() {
    let capped = cap_utf8("abcédef".into(), 4);

    assert!(capped.starts_with("abc"));
    assert!(capped.contains("patch truncated at 4 bytes"));
}

#[test]
fn summary_helpers_extract_deploy_ci_and_receipts() {
    let event = JailgunEvent::new("run-1", EventKind::DeployFinished, "deploy finished")
        .with_field("outcome", "dry-run-staged")
        .with_field("ci_state", "skipped")
        .with_field("receipt_path", "target/receipts/deploy.json");
    let events = vec![event];

    assert_eq!(deploy_status_from_events(&events, true), "dry-run-staged");
    assert_eq!(ci_status_from_events(&events, true), "skipped");
    assert_eq!(
        receipt_paths_from_events(&events),
        vec![PathBuf::from("target/receipts/deploy.json")]
    );
}

#[test]
fn summary_accepts_direct_tex_download_without_tar_validation() {
    let root = tempfile::tempdir().expect("tempdir");
    let tex_path = root.path().join("chapter-033-epoch-02.tex");
    std::fs::write(&tex_path, "\\chapter{Test}\n\nBody.\n").expect("write tex");
    let event = JailgunEvent::new("run-1", EventKind::DownloadReceipt, "download complete")
        .with_tab(1)
        .with_field("local_path", tex_path.display().to_string())
        .with_field(
            "receipt_path",
            root.path().join("receipt.json").display().to_string(),
        )
        .with_field("sha256", "a".repeat(64))
        .with_field("size_bytes", "24")
        .with_field("file_kind", "downloaded-tex");

    let (artifacts, failures) =
        artifacts_from_events(&[event], &jailgun_core::JailgunConfig::default(), None);

    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].kind, "downloaded-tex");
    assert!(artifacts[0].tar_validation.is_none());
    assert!(artifacts[0].changed_files.is_empty());
}

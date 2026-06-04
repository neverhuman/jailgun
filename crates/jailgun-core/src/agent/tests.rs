use std::path::PathBuf;

use super::*;

fn request() -> JailgunAgentRunRequest {
    JailgunAgentRunRequest {
        version: JAILGUN_AGENT_INTERFACE_VERSION,
        run_id: Some("run-1".into()),
        prompt_ref: "jmcp://work-orders/1/prompt".into(),
        prompt_file: PathBuf::from("/tmp/prompt.txt"),
        config_path: None,
        tabs: Some(2),
        max_runtime_seconds: Some(120),
        repo: JailgunRepoRef::default(),
        source_archive: JailgunSourceArchiveRequest::default(),
        deploy: JailgunAgentDeployRequest::default(),
        ci: JailgunCiRequest::default(),
        browser: JailgunAgentBrowserRequest::default(),
        github: JailgunGithubPolicyRequest::default(),
    }
}

#[test]
fn request_defaults_to_dry_run_deploy_off() {
    let value: JailgunAgentRunRequest = serde_json::from_value(serde_json::json!({
        "prompt_ref": "jmcp://prompt/1",
        "prompt_file": "/tmp/prompt.txt"
    }))
    .expect("request");

    assert!(!value.deploy.enabled);
    assert!(value.deploy.dry_run);
    assert!(!value.deploy.allow_live);
    value.validate_for_config_tabs(1).expect("valid default");
}

#[test]
fn request_rejects_runtime_and_tab_caps() {
    let mut value = request();
    value.tabs = Some(JAILGUN_AGENT_MAX_TABS + 1);
    assert!(value
        .validate_for_config_tabs(1)
        .unwrap_err()
        .contains("tabs"));

    value.tabs = Some(1);
    value.max_runtime_seconds = Some(JAILGUN_AGENT_MAX_RUNTIME_SECONDS + 1);
    assert!(value
        .validate_for_config_tabs(1)
        .unwrap_err()
        .contains("max_runtime_seconds"));
}

#[test]
fn request_rejects_live_deploy_without_explicit_allow() {
    let mut value = request();
    value.deploy.enabled = true;
    value.deploy.dry_run = false;

    assert!(value
        .validate_for_config_tabs(1)
        .unwrap_err()
        .contains("allow_live"));

    value.deploy.allow_live = true;
    value
        .validate_for_config_tabs(1)
        .expect("explicit live allow");
}

#[test]
fn request_rejects_path_like_run_ids() {
    let mut value = request();
    value.run_id = Some("../outside".into());
    assert!(value
        .validate_for_config_tabs(1)
        .unwrap_err()
        .contains("run_id"));

    value.run_id = Some("run_ok-1.2".into());
    value.validate_for_config_tabs(1).expect("safe run id");
}

#[test]
fn request_requires_repo_scope_for_github_write_allow() {
    let mut value = request();
    value.github.allow_write_prompts = true;

    assert!(value
        .validate_for_config_tabs(1)
        .unwrap_err()
        .contains("allowed_repositories"));

    value.github.allowed_repositories = vec!["org/example".into()];
    value
        .validate_for_config_tabs(1)
        .expect("repo scoped allow");
}

#[test]
fn summary_shape_excludes_prompt_text() {
    let secret_prompt = "implement private customer request";
    let summary = JailgunAgentRunSummary {
        version: JAILGUN_AGENT_INTERFACE_VERSION,
        run_id: "run-1".into(),
        status: "succeeded".into(),
        prompt_ref: "jmcp://prompt/1".into(),
        tab_count: 1,
        max_runtime_seconds: 60,
        repo_ref: JailgunRepoRef::default(),
        source_archive: JailgunSourceArchiveSummary {
            enabled: false,
            repo_url: "git@example.com:org/repo.git".into(),
            ref_name: "HEAD".into(),
            prefix: "source/".into(),
            archive_filename: "source.tar.gz".into(),
        },
        deploy_status: "disabled".into(),
        ci_status: "disabled".into(),
        changed_files: Vec::new(),
        artifacts: Vec::new(),
        failures: vec![JailgunFailure {
            tab_id: None,
            code: "example".into(),
            message: "safe failure".into(),
        }],
        events_jsonl: PathBuf::from("events.jsonl"),
        receipt_paths: Vec::new(),
        started_at: "2026-06-01T00:00:00Z".into(),
        finished_at: "2026-06-01T00:00:01Z".into(),
        denied_github_prompts: 0,
        allowed_info_prompts: 0,
        github_write_prompts_allowed: false,
    };

    let json = serde_json::to_string(&summary).expect("summary json");
    assert!(!json.contains(secret_prompt));
    assert!(json.contains("prompt_ref"));
    assert!(!json.contains("prompt_text"));
}

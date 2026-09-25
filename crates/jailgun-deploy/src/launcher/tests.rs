use super::*;
use crate::{job::JobPhase, job::JobSpec};

fn sample_spec() -> JobSpec {
    JobSpec {
        run_id: "run-fixture".into(),
        tab_id: 1,
        remote_dir: "/srv/example-project".into(),
        remote_archive_path: "/tmp/jailgun-runs/run-fixture-tab-01/uploads/x.tar.gz".into(),
        remote_command: "bash ci-fast-push.sh".into(),
        strip_components: 1,
        local_sha256: "a".repeat(64),
        remote_sha256: "a".repeat(64),
        stash_on_failure: true,
    }
}

#[test]
fn launcher_script_has_stable_shape() {
    let body = build_launcher_script(&sample_spec());
    assert!(body.starts_with("#!/usr/bin/env bash"));
    assert!(body.contains("jailgun-launcher schema v1"));
    assert!(body.contains("write_status \"queued\""));
    assert!(body.contains("write_status \"done\""));
    assert!(body.contains("preserve_and_reset"));
    assert!(body.contains("collect_commit_stats"));
    assert!(body.contains("JOB_ID='run-fixture-tab-01'"));
    assert!(body.contains("STASH_ON_FAILURE=1"));
    assert!(body.contains("STRIP_COMPONENTS=1"));
}

#[test]
fn launcher_quotes_remote_command_with_special_chars() {
    let mut spec = sample_spec();
    spec.remote_command = "echo it's $HOME".into();
    let body = build_launcher_script(&spec);
    assert!(body.contains("REMOTE_COMMAND='echo it'\\''s $HOME'"));
}

#[test]
fn parse_status_handles_full_payload() {
    let raw = r#"{
        "schema_version": 1,
        "phase": "done",
        "exit_code": 0,
        "pre_head": "abc",
        "post_head": "def",
        "preserved_ref": null,
        "preserved_sha": null,
        "preserved_stash": null,
        "preserved_stash_ref": null,
        "preserved_patch_path": null,
        "reset_to": null,
        "reset_ok": null,
        "failure_reason": null,
        "files_changed": 3,
        "additions": 10,
        "deletions": 2,
        "top_paths": ["src/main.rs", "README.md"],
        "started_at": "2026-05-31T12:00:00Z",
        "finished_at": "2026-05-31T12:01:00Z",
        "failed_at": null
    }"#;
    let parsed = parse_status_json(raw).expect("parses");
    assert_eq!(parsed.phase, JobPhase::Done);
    assert_eq!(parsed.exit_code, Some(0));
    assert_eq!(parsed.files_changed, Some(3));
    assert_eq!(parsed.top_paths, vec!["src/main.rs", "README.md"]);
    assert!(parsed.raw.is_object());
}

#[test]
fn parse_status_handles_missing_status_phase() {
    let parsed = parse_status_json(r#"{"phase":"missing-status"}"#).expect("parses");
    assert_eq!(parsed.phase, JobPhase::MissingStatus);
    assert!(!parsed.phase.is_terminal());
    assert!(!parsed.phase.is_success());
}

#[test]
fn parse_status_handles_failed_preserved_phase() {
    let parsed = parse_status_json(
        r#"{"phase":"failed-preserved","preserved_ref":"jailgun-failed/x","reset_ok":true}"#,
    )
    .expect("parses");
    assert_eq!(parsed.phase, JobPhase::FailedPreserved);
    assert_eq!(parsed.preserved_ref.as_deref(), Some("jailgun-failed/x"));
    assert_eq!(parsed.reset_ok, Some(true));
    assert!(parsed.phase.is_terminal());
    assert!(!parsed.phase.is_success());
}

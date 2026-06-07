use std::{path::PathBuf, time::Duration};

use crate::{config::RunOptions, support::deploy_outcome_succeeded};

use super::bridge_flow::{open_tab_profile_dir, prompt_for_tab};
use super::launch::LaunchScheduler;
use super::timing::run_deadline;
use super::tracker::RunTracker;
use super::*;

#[test]
fn prompt_for_tab_prefixes_and_replaces_placeholders() {
    let prompt = "Build {{TAB_INDEX}} of {{TAB_COUNT}}.";
    let got = prompt_for_tab(prompt, 2, 7);
    assert!(got.starts_with("Batch tab: 2 of 7."));
    assert!(got.contains("Do not answer with the tab number by itself."));
    assert!(got.contains("Build 2 of 7."));
}

#[test]
fn launch_scheduler_waits_for_prompt_acceptance_before_next_tab() {
    let mut scheduler = LaunchScheduler::new(3);
    assert_eq!(scheduler.next_tab, 1);
    scheduler.next_tab = 2;
    scheduler.waiting_for_acceptance = Some(1);

    assert!(scheduler
        .prompt_accepted(2, Duration::from_secs(60))
        .is_none());
    let delay = scheduler
        .prompt_accepted(1, Duration::from_secs(60))
        .expect("tab 2 scheduled after tab 1 acceptance");
    assert_eq!(delay.tab_id, 2);
    assert_eq!(delay.duration, Duration::from_secs(60));
    assert!(scheduler.consume_scheduled_launch(2));
}

#[test]
fn run_tracker_completes_when_a_tab_fails_before_download() {
    let mut tracker = RunTracker::new(2, true);
    tracker.mark_terminal(1);
    assert!(!tracker.is_complete());
    tracker.mark_downloaded(2);
    assert!(!tracker.is_complete());
    tracker.mark_deployed(2);
    assert!(tracker.is_complete());
}

#[test]
fn run_deadline_honors_explicit_runtime_cap() {
    let mut opts = RunOptions {
        run_id: "run-1".into(),
        config: jailgun_core::JailgunConfig::default(),
        prompt_text: "prompt".into(),
        tabs_override: Some(5),
        no_deploy: true,
        dry_run: true,
        profile_dir: PathBuf::from("/tmp/profile"),
        profile_pool: Vec::new(),
        tab_profile_dirs: Default::default(),
        downloads_dir: PathBuf::from("/tmp/downloads"),
        artifacts_dir: PathBuf::from("/tmp/artifacts"),
        bridge_cmd: vec!["bridge".into()],
        bridge_env: Default::default(),
        repo_url: "git@example.com:org/repo.git".into(),
        local_archive_path: None,
        deploy_remote_host: None,
        deploy_remote_dir: None,
        deploy_remote_command: None,
        deploy_expected_top_level: None,
        ci_tracker_enabled: false,
        ci_repo: None,
        ci_branch: "main".into(),
        ci_max_attempts: 1,
        ci_poll_seconds: 1,
        status_max_minutes: 1,
        max_runtime_seconds: Some(120),
        event_buffer: 64,
        deploy_concurrency: 1,
    };

    assert_eq!(run_deadline(&opts, 5), Duration::from_secs(120));
    opts.max_runtime_seconds = None;
    assert!(run_deadline(&opts, 5) > Duration::from_secs(120));
}

#[tokio::test]
async fn run_orchestration_rejects_path_like_run_id() {
    let opts = RunOptions {
        run_id: "../outside".into(),
        config: jailgun_core::JailgunConfig::default(),
        prompt_text: "prompt".into(),
        tabs_override: Some(1),
        no_deploy: true,
        dry_run: true,
        profile_dir: PathBuf::from("/tmp/profile"),
        profile_pool: Vec::new(),
        tab_profile_dirs: Default::default(),
        downloads_dir: PathBuf::from("/tmp/downloads"),
        artifacts_dir: PathBuf::from("/tmp/artifacts"),
        bridge_cmd: vec!["bridge".into()],
        bridge_env: Default::default(),
        repo_url: "git@example.com:org/repo.git".into(),
        local_archive_path: None,
        deploy_remote_host: None,
        deploy_remote_dir: None,
        deploy_remote_command: None,
        deploy_expected_top_level: None,
        ci_tracker_enabled: false,
        ci_repo: None,
        ci_branch: "main".into(),
        ci_max_attempts: 1,
        ci_poll_seconds: 1,
        status_max_minutes: 1,
        max_runtime_seconds: Some(120),
        event_buffer: 64,
        deploy_concurrency: 1,
    };

    match run_orchestration(opts).await {
        Ok(_) => panic!("path-like run_id accepted"),
        Err(error) => assert!(error.to_string().contains("run_id")),
    }
}

#[test]
fn open_tab_profile_dir_is_omitted_for_profile_pool() {
    let mut opts = RunOptions {
        run_id: "run-1".into(),
        config: jailgun_core::JailgunConfig::default(),
        prompt_text: "prompt".into(),
        tabs_override: Some(2),
        no_deploy: true,
        dry_run: true,
        profile_dir: PathBuf::from("/tmp/profile-a"),
        profile_pool: vec![
            PathBuf::from("/tmp/profile-a"),
            PathBuf::from("/tmp/profile-b"),
        ],
        tab_profile_dirs: Default::default(),
        downloads_dir: PathBuf::from("/tmp/downloads"),
        artifacts_dir: PathBuf::from("/tmp/artifacts"),
        bridge_cmd: vec!["bridge".into()],
        bridge_env: Default::default(),
        repo_url: "git@example.com:org/repo.git".into(),
        local_archive_path: None,
        deploy_remote_host: None,
        deploy_remote_dir: None,
        deploy_remote_command: None,
        deploy_expected_top_level: None,
        ci_tracker_enabled: false,
        ci_repo: None,
        ci_branch: "main".into(),
        ci_max_attempts: 1,
        ci_poll_seconds: 1,
        status_max_minutes: 1,
        max_runtime_seconds: Some(120),
        event_buffer: 64,
        deploy_concurrency: 1,
    };
    assert_eq!(open_tab_profile_dir(&opts, 1), None);

    opts.profile_pool.clear();
    assert_eq!(
        open_tab_profile_dir(&opts, 1),
        Some("/tmp/profile-a".to_string())
    );
    opts.tab_profile_dirs
        .insert(2, PathBuf::from("/tmp/profile-b"));
    assert_eq!(
        open_tab_profile_dir(&opts, 2),
        Some("/tmp/profile-b".to_string())
    );
}

#[test]
fn failed_preserved_deploy_outcome_is_not_successful() {
    assert!(deploy_outcome_succeeded(
        jailgun_deploy::DeployOutcome::Succeeded
    ));
    assert!(deploy_outcome_succeeded(
        jailgun_deploy::DeployOutcome::SucceededCiSkipped
    ));
    assert!(!deploy_outcome_succeeded(
        jailgun_deploy::DeployOutcome::FailedPreserved
    ));
    assert!(!deploy_outcome_succeeded(
        jailgun_deploy::DeployOutcome::SucceededCiFailed
    ));
}

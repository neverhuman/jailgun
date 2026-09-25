use jailgun_core::TarValidation;
use jailgun_deploy::DeployOutcome;
use jailgun_orchestrator::support::{
    deploy_outcome_succeeded, ensure_expected_top_level, infer_github_repo,
};

use super::{deploy::resolve_deploy_dry_run, run::validated_run_id};
fn validation_with_top_level(top_level: Option<&str>) -> TarValidation {
    TarValidation {
        size_bytes: 1,
        entry_count: 1,
        files: Vec::new(),
        top_levels: top_level
            .map(|value| vec![value.to_string()])
            .unwrap_or_else(|| vec!["jekko".to_string(), "other".to_string()]),
        top_level: top_level.map(str::to_string),
    }
}

#[test]
fn deploy_archive_accepts_expected_top_level() {
    let validation = validation_with_top_level(Some("jekko"));
    ensure_expected_top_level(&validation, "jekko").unwrap();
}

#[test]
fn deploy_archive_rejects_jekko_fixes_top_level() {
    let validation = validation_with_top_level(Some("jekko-fixes"));
    let error = ensure_expected_top_level(&validation, "jekko").unwrap_err();
    assert!(error
        .to_string()
        .contains("archive top-level must be jekko/, found jekko-fixes"));
}

#[test]
fn deploy_archive_rejects_multiple_top_levels() {
    let validation = validation_with_top_level(None);
    let error = ensure_expected_top_level(&validation, "jekko").unwrap_err();
    assert!(error
        .to_string()
        .contains("archive top-level must be jekko/, found (multiple)"));
}

#[test]
fn run_deploy_flag_disables_config_dry_run() {
    assert!(!resolve_deploy_dry_run(true, true, false));
}

#[test]
fn run_dry_run_flag_overrides_deploy() {
    assert!(resolve_deploy_dry_run(false, true, true));
}

#[test]
fn run_without_deploy_preserves_config_dry_run() {
    assert!(resolve_deploy_dry_run(true, false, false));
    assert!(!resolve_deploy_dry_run(false, false, false));
}

#[test]
fn direct_run_rejects_path_like_run_id() {
    let error = validated_run_id(Some("../outside".into())).unwrap_err();
    assert!(error.to_string().contains("run_id"));
}

#[test]
fn failed_preserved_deploy_archive_outcome_is_not_successful() {
    assert!(deploy_outcome_succeeded(DeployOutcome::Succeeded));
    assert!(deploy_outcome_succeeded(DeployOutcome::SucceededCiSkipped));
    assert!(!deploy_outcome_succeeded(DeployOutcome::FailedPreserved));
    assert!(!deploy_outcome_succeeded(DeployOutcome::SucceededCiFailed));
}

#[test]
fn github_repo_inference_stays_within_owner_repo_boundary() {
    assert_eq!(
        infer_github_repo("git@github.com:example/repo.git").as_deref(),
        Some("example/repo")
    );
    assert_eq!(
        infer_github_repo("https://github.com/example/repo.git").as_deref(),
        Some("example/repo")
    );
    assert_eq!(
        infer_github_repo("ssh://git@github.com/example/repo.git").as_deref(),
        Some("example/repo")
    );
    assert_eq!(infer_github_repo("git@example.com:org/repo.git"), None);
    assert_eq!(
        infer_github_repo("https://notgithub.com/example/repo.git"),
        None
    );
    assert_eq!(
        infer_github_repo("https://github.com/example/repo/tree/main"),
        None
    );
}

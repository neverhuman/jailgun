use std::path::PathBuf;

use anyhow::{Context, Result};
use jailgun_core::{validate_tar_gz, CleanupPolicy, JailgunConfig};
use jailgun_deploy::{
    cleanup_remote_checkout, deploy_remote,
    shell::{SshCiTracker, SshRemoteGit, SshRemoteJob, SshRemoteUpload},
    CleanupRequest, DeployRequest,
};

use crate::cli::CleanupPolicyArg;
use jailgun_orchestrator::support::{
    arg_or_env, deploy_outcome_label, deploy_outcome_succeeded, deploy_remote_command,
    ensure_expected_top_level, infer_github_repo, LocalReceiptWriter,
};

#[allow(clippy::too_many_arguments)]
pub(super) async fn remote_cleanup(
    config: PathBuf,
    run_id: String,
    tab_id: Option<u16>,
    remote_host: Option<String>,
    remote_dir: Option<String>,
    receipt_dir: Option<PathBuf>,
    policy: Option<CleanupPolicyArg>,
) -> Result<()> {
    let config = JailgunConfig::from_toml_path(&config)
        .with_context(|| format!("loading {}", config.display()))?;
    let remote_host = arg_or_env(remote_host, &config.deploy.remote_host_env, "remote host")?;
    let remote_dir = arg_or_env(remote_dir, &config.deploy.remote_dir_env, "remote dir")?;
    let receipt_dir =
        receipt_dir.unwrap_or_else(|| PathBuf::from(&config.paths.artifacts_dir).join("receipts"));
    let policy = policy
        .map(CleanupPolicy::from)
        .unwrap_or(config.deploy.remote_cleanup_policy);
    let mut backend = SshRemoteGit::new(remote_host.clone(), receipt_dir.clone());
    let receipt = cleanup_remote_checkout(
        &mut backend,
        CleanupRequest {
            run_id,
            tab_id,
            remote_host,
            remote_dir,
            policy,
            receipt_dir,
        },
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn deploy_archive(
    archive: PathBuf,
    config: PathBuf,
    run_id: String,
    tab_id: u16,
    remote_host: Option<String>,
    remote_dir: Option<String>,
    remote_command: Option<String>,
    receipt_dir: Option<PathBuf>,
    policy: Option<CleanupPolicyArg>,
    dry_run: bool,
    expected_top_level: String,
    status_max_minutes: u16,
    ci: bool,
    ci_repo: Option<String>,
) -> Result<()> {
    let config = JailgunConfig::from_toml_path(&config)
        .with_context(|| format!("loading {}", config.display()))?;
    let ci_repo = ci_repo.or_else(|| infer_github_repo(&config.project.repository));
    let remote_host = arg_or_env(remote_host, &config.deploy.remote_host_env, "remote host")?;
    let remote_dir = arg_or_env(remote_dir, &config.deploy.remote_dir_env, "remote dir")?;
    let remote_command = deploy_remote_command(remote_command, &config.deploy.remote_command_env)?;
    let receipt_dir =
        receipt_dir.unwrap_or_else(|| PathBuf::from(&config.paths.artifacts_dir).join("receipts"));
    let policy = policy
        .map(CleanupPolicy::from)
        .unwrap_or(config.deploy.remote_cleanup_policy);
    let validation = validate_tar_gz(&archive, config.deploy.remote_strip_components > 0)
        .with_context(|| format!("validating {}", archive.display()))?;
    ensure_expected_top_level(&validation, &expected_top_level)?;
    eprintln!(
        "validated archive: {} bytes, {} entries, top_level={}",
        validation.size_bytes,
        validation.entry_count,
        validation.top_level.as_deref().unwrap_or("(multiple)")
    );

    let mut git = SshRemoteGit::new(remote_host.clone(), receipt_dir.clone());
    let cleanup = cleanup_remote_checkout(
        &mut git,
        CleanupRequest {
            run_id: run_id.clone(),
            tab_id: Some(tab_id),
            remote_host: remote_host.clone(),
            remote_dir: remote_dir.clone(),
            policy,
            receipt_dir: receipt_dir.clone(),
        },
    )
    .await?;
    eprintln!(
        "remote cleanup outcome: {:?} preserved_ref={}",
        cleanup.outcome,
        cleanup.preserved_ref.as_deref().unwrap_or("-")
    );

    let archive_name = archive
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("chatgpt-output.tar.gz")
        .to_string();
    let (events, _rx) = tokio::sync::broadcast::channel(128);
    let mut upload = SshRemoteUpload::new(remote_host.clone());
    let mut job = SshRemoteJob::new(remote_host.clone());
    let mut ci_tracker = SshCiTracker::with_repo(ci_repo.clone());
    let mut writer = LocalReceiptWriter::new(receipt_dir.clone());
    let receipt = deploy_remote(
        &mut upload,
        &mut job,
        &mut ci_tracker,
        &mut writer,
        DeployRequest {
            run_id,
            tab_id,
            remote_host,
            remote_dir,
            remote_command,
            remote_archive_basename: archive_name,
            local_archive_path: archive,
            strip_components: config.deploy.remote_strip_components,
            cleanup_policy: policy,
            receipt_dir,
            status_poll_seconds: config.deploy.remote_status_poll_seconds,
            status_max_minutes,
            ci_tracker_enabled: ci,
            ci_repo,
            ci_branch: "main".into(),
            ci_max_attempts: 20,
            ci_poll_seconds: 30,
            stash_on_failure: true,
            dry_run,
        },
        &events,
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    if !deploy_outcome_succeeded(receipt.outcome) {
        let mut reason = format!(
            "deploy finished with outcome {}",
            deploy_outcome_label(receipt.outcome)
        );
        if let Some(failure_reason) = receipt.final_status.failure_reason.as_deref() {
            reason.push_str(&format!("; failure_reason={failure_reason}"));
        }
        if let Some(exit_code) = receipt.final_status.exit_code {
            reason.push_str(&format!("; exit_code={exit_code}"));
        }
        if let Some(line) = receipt
            .log_tail
            .lines()
            .find(|line| !line.trim().is_empty())
        {
            reason.push_str(&format!("; log_tail={}", line.trim()));
        }
        anyhow::bail!(reason);
    }
    Ok(())
}

pub(super) fn resolve_deploy_dry_run(config_dry_run: bool, deploy: bool, dry_run: bool) -> bool {
    if dry_run {
        true
    } else if deploy {
        false
    } else {
        config_dry_run
    }
}

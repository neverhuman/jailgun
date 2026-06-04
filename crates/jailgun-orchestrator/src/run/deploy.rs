use std::path::PathBuf;

use jailgun_core::JailgunEvent;
use jailgun_deploy::{
    cleanup_remote_checkout,
    shell::{SshCiTracker, SshRemoteGit, SshRemoteJob, SshRemoteUpload},
    CleanupRequest, DeployRequest,
};
use tokio::sync::broadcast;

use crate::{
    config::RunOptions,
    support::{
        deploy_outcome_label, deploy_outcome_succeeded, ensure_expected_top_level,
        LocalReceiptWriter,
    },
};

pub(super) async fn deploy_download(
    opts: &RunOptions,
    events: &broadcast::Sender<JailgunEvent>,
    tab_id: u16,
    local_path: String,
    local_name: String,
) -> Result<(), String> {
    let archive_path = PathBuf::from(&local_path);
    let require_single_top_level = opts.config.deploy.remote_strip_components > 0;
    let validation = jailgun_core::validate_tar_gz(&archive_path, require_single_top_level)
        .map_err(|error| error.to_string())?;
    if let Some(expected) = opts.deploy_expected_top_level.as_deref() {
        if validation.top_level.as_deref() != Some(expected) {
            return Err(format!(
                "archive top-level must be {expected}/, found {}; refusing remote upload",
                validation.top_level.as_deref().unwrap_or("(multiple)")
            ));
        }
    }

    let remote_host = opts
        .deploy_remote_host
        .clone()
        .ok_or_else(|| "deploy remote host is not configured".to_string())?;
    let remote_dir = opts
        .deploy_remote_dir
        .clone()
        .ok_or_else(|| "deploy remote dir is not configured".to_string())?;
    let remote_command = opts
        .deploy_remote_command
        .clone()
        .unwrap_or_else(|| "bash ci-fast-push.sh".into());
    let receipt_dir = opts.artifacts_dir.join("receipts").join(&opts.run_id);
    let archive_name = if local_name.trim().is_empty() {
        archive_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("chatgpt-output.tar.gz")
            .to_string()
    } else {
        local_name
    };

    let mut git = SshRemoteGit::new(remote_host.clone(), receipt_dir.clone());
    cleanup_remote_checkout(
        &mut git,
        CleanupRequest {
            run_id: opts.run_id.clone(),
            tab_id: Some(tab_id),
            remote_host: remote_host.clone(),
            remote_dir: remote_dir.clone(),
            policy: opts.config.deploy.remote_cleanup_policy,
            receipt_dir: receipt_dir.clone(),
        },
    )
    .await
    .map_err(|error| error.to_string())?;

    let mut upload = SshRemoteUpload::new(remote_host.clone());
    let mut job = SshRemoteJob::new(remote_host.clone());
    let mut ci = SshCiTracker::with_repo(opts.ci_repo.clone());
    let mut writer = LocalReceiptWriter::new(receipt_dir.clone());
    let receipt = jailgun_deploy::deploy_remote(
        &mut upload,
        &mut job,
        &mut ci,
        &mut writer,
        DeployRequest {
            run_id: opts.run_id.clone(),
            tab_id,
            remote_host,
            remote_dir,
            remote_command,
            remote_archive_basename: archive_name,
            local_archive_path: archive_path,
            strip_components: opts.config.deploy.remote_strip_components,
            cleanup_policy: opts.config.deploy.remote_cleanup_policy,
            receipt_dir,
            status_poll_seconds: opts.config.deploy.remote_status_poll_seconds,
            status_max_minutes: opts.status_max_minutes,
            ci_tracker_enabled: opts.ci_tracker_enabled,
            ci_repo: opts.ci_repo.clone(),
            ci_branch: opts.ci_branch.clone(),
            ci_max_attempts: opts.ci_max_attempts,
            ci_poll_seconds: opts.ci_poll_seconds,
            stash_on_failure: true,
            dry_run: opts.dry_run || opts.config.deploy.dry_run,
        },
        events,
    )
    .await
    .map_err(|error| error.to_string())?;

    if deploy_outcome_succeeded(receipt.outcome) {
        Ok(())
    } else {
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
        Err(reason)
    }
}

pub(super) fn validate_download_archive(
    opts: &RunOptions,
    tab_id: u16,
    local_path: &str,
) -> Result<(), String> {
    let archive_path = PathBuf::from(local_path);
    let require_single_top_level =
        opts.config.deploy.remote_strip_components > 0 || opts.deploy_expected_top_level.is_some();
    let validation = jailgun_core::validate_tar_gz(&archive_path, require_single_top_level)
        .map_err(|error| error.to_string())?;
    if validation.entry_count == 0 {
        return Err(format!(
            "tab {tab_id} downloaded archive has zero tar entries: {}",
            archive_path.display()
        ));
    }
    if let Some(expected) = opts.deploy_expected_top_level.as_deref() {
        ensure_expected_top_level(&validation, expected).map_err(|error| error.to_string())?;
    }
    Ok(())
}

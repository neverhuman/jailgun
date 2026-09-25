//! Remote deploy orchestrator.
//!
//! `deploy_remote` is the free function that ties together the four trait
//! families (`RemoteUploadBackend`, `RemoteJobBackend`, `CiTracker`,
//! `JsonReceiptWriter`) plus the broadcast event bus. Production wiring uses
//! the SSH backends in `shell.rs`; tests use the fakes in `#[cfg(test)]` here
//! and (for cross-crate use) the `fake-backends` Cargo feature module
//! `crate::fake`.

use jailgun_core::{EventKind, JailgunEvent, Severity};
use tokio::sync::broadcast;

mod events;
mod model;
mod receipt;
mod runtime;

pub use model::{DeployError, DeployOutcome, DeployReceipt, DeployRequest, JsonReceiptWriter};

use events::{publish, publish_error, publish_finished};
use receipt::build_receipt;
use runtime::{poll_until_terminal, timestamp_now, track_ci};

use crate::{
    ci::{CiState, CiTracker},
    job::{JobPhase, JobSpec, JobStatus, RemoteJobBackend},
    upload::RemoteUploadBackend,
    util::truncate_log_tail,
};

pub async fn deploy_remote<U, J, C, W>(
    upload: &mut U,
    job: &mut J,
    ci: &mut C,
    writer: &mut W,
    req: DeployRequest,
    events: &broadcast::Sender<JailgunEvent>,
) -> Result<DeployReceipt, DeployError>
where
    U: RemoteUploadBackend + Send,
    J: RemoteJobBackend + Send,
    C: CiTracker + Send,
    W: JsonReceiptWriter + Send,
{
    let started_at = timestamp_now();
    let local_sha256 = jailgun_core::sha256_file(&req.local_archive_path)?;

    let mut queued_event =
        JailgunEvent::new(req.run_id.clone(), EventKind::DeployQueued, "deploy queued")
            .with_tab(req.tab_id)
            .with_field("local_sha256", local_sha256.clone())
            .with_field(
                "local_archive_path",
                req.local_archive_path.display().to_string(),
            )
            .with_field("remote_host", req.remote_host.clone())
            .with_field("remote_dir", req.remote_dir.clone());
    if let Some(ci_repo) = req.ci_repo.as_ref() {
        queued_event = queued_event.with_field("ci_repo", ci_repo.clone());
    }
    publish(events, queued_event);

    let job_id = format!(
        "{}-tab-{:02}",
        crate::util::sanitize_ref_fragment(&req.run_id),
        req.tab_id
    );
    let upload_dir = format!("/tmp/jailgun-runs/{job_id}/uploads");
    let remote_archive_path = format!("{upload_dir}/{}", req.remote_archive_basename);

    upload
        .ensure_remote_dir(&upload_dir)
        .await
        .inspect_err(|error| {
            publish_error(events, &req, "ensure_remote_dir", error);
        })?;

    if let Err(error) = upload
        .upload_archive(&req.local_archive_path, &remote_archive_path)
        .await
    {
        publish_error(events, &req, "upload_archive", &error);
        return Err(error);
    }

    let remote_sha256 = upload.remote_sha256(&remote_archive_path).await?;
    if remote_sha256 != local_sha256 {
        let _ = upload.remove_remote_file(&remote_archive_path).await;
        publish(
            events,
            JailgunEvent::new(
                req.run_id.clone(),
                EventKind::DeployFinished,
                "remote sha mismatch",
            )
            .with_tab(req.tab_id)
            .with_severity(Severity::Error)
            .with_field("local_sha256", local_sha256.clone())
            .with_field("remote_sha256", remote_sha256.clone())
            .with_field("outcome", "upload-sha-mismatch"),
        );
        return Err(DeployError::ShaMismatch {
            local: local_sha256,
            remote: remote_sha256,
        });
    }

    publish(
        events,
        JailgunEvent::new(
            req.run_id.clone(),
            EventKind::RemoteSafety,
            "upload verified",
        )
        .with_tab(req.tab_id)
        .with_field("phase", "upload-verified")
        .with_field("remote_sha256", remote_sha256.clone()),
    );

    let spec = JobSpec {
        run_id: req.run_id.clone(),
        tab_id: req.tab_id,
        remote_dir: req.remote_dir.clone(),
        remote_archive_path: remote_archive_path.clone(),
        remote_command: req.remote_command.clone(),
        strip_components: req.strip_components,
        local_sha256: local_sha256.clone(),
        remote_sha256: remote_sha256.clone(),
        stash_on_failure: req.stash_on_failure,
    };

    let handle = job.install_launcher(&spec).await.inspect_err(|error| {
        publish_error(events, &req, "install_launcher", error);
    })?;

    if req.dry_run {
        let receipt = build_receipt(
            &req,
            started_at,
            timestamp_now(),
            local_sha256,
            remote_sha256,
            remote_archive_path,
            handle,
            JobStatus {
                phase: JobPhase::Queued,
                ..Default::default()
            },
            CiState::Skipped {
                reason: "dry-run".into(),
            },
            String::new(),
            DeployOutcome::DryRunStaged,
        );
        let path = writer.write_receipt(&receipt).await?;
        let mut receipt = receipt;
        receipt.receipt_path = Some(path);
        publish_finished(events, &req, &receipt);
        return Ok(receipt);
    }

    job.start_job(&spec, &handle).await.inspect_err(|error| {
        publish_error(events, &req, "start_job", error);
    })?;

    let final_status = poll_until_terminal(job, &handle, &req, events).await?;
    let log_tail = match job.fetch_log_tail(&handle, 40).await {
        Ok(text) => truncate_log_tail(&text, 20, 4096),
        Err(error) => {
            tracing::warn!(
                ?error,
                run_id = %req.run_id,
                tab_id = req.tab_id,
                "log tail fetch failed; receipt will record empty tail"
            );
            String::new()
        }
    };

    let mut status = final_status;
    status.log_tail = Some(log_tail.clone());

    let outcome_pre_ci = match status.phase {
        JobPhase::Done => DeployOutcome::Succeeded,
        JobPhase::FailedPreserved => DeployOutcome::FailedPreserved,
        JobPhase::Failed => DeployOutcome::FailedHard,
        _ => DeployOutcome::TimedOut,
    };

    let post_head_nonempty = matches!(status.post_head.as_deref(), Some(h) if !h.is_empty());
    let ci_state = if matches!(outcome_pre_ci, DeployOutcome::Succeeded)
        && req.ci_tracker_enabled
        && post_head_nonempty
        && status.pre_head != status.post_head
    {
        track_ci(ci, &status, &req, events).await
    } else if matches!(outcome_pre_ci, DeployOutcome::Succeeded) {
        CiState::Skipped {
            reason: "no-commit-change-or-disabled".into(),
        }
    } else {
        CiState::Unknown
    };

    let outcome = match (outcome_pre_ci, &ci_state) {
        (DeployOutcome::Succeeded, CiState::Failed { .. }) => DeployOutcome::SucceededCiFailed,
        (DeployOutcome::Succeeded, CiState::Skipped { .. }) => DeployOutcome::SucceededCiSkipped,
        (other, _) => other,
    };

    let mut receipt = build_receipt(
        &req,
        started_at,
        timestamp_now(),
        local_sha256,
        remote_sha256,
        remote_archive_path,
        handle,
        status,
        ci_state,
        log_tail,
        outcome,
    );
    let path = writer.write_receipt(&receipt).await?;
    receipt.receipt_path = Some(path);

    publish_finished(events, &req, &receipt);
    Ok(receipt)
}

use jailgun_core::{EventKind, JailgunEvent, Severity};
use tokio::sync::broadcast;

use super::{DeployOutcome, DeployReceipt, DeployRequest};
use crate::{
    ci::CiState,
    job::{JobPhase, JobStatus},
};

pub(super) fn publish(events: &broadcast::Sender<JailgunEvent>, event: JailgunEvent) {
    let _ = events.send(event);
}

pub(super) fn publish_error<E: std::fmt::Display>(
    events: &broadcast::Sender<JailgunEvent>,
    req: &DeployRequest,
    phase: &str,
    error: &E,
) {
    publish(
        events,
        JailgunEvent::new(
            req.run_id.clone(),
            EventKind::Error,
            format!("deploy step {phase} failed"),
        )
        .with_tab(req.tab_id)
        .with_severity(Severity::Error)
        .with_field("phase", phase.to_string())
        .with_field("error", error.to_string()),
    );
}

pub(super) fn publish_status_progress(
    events: &broadcast::Sender<JailgunEvent>,
    req: &DeployRequest,
    status: &JobStatus,
) {
    let mut event = JailgunEvent::new(
        req.run_id.clone(),
        EventKind::RemoteSafety,
        "deploy progress",
    )
    .with_tab(req.tab_id)
    .with_field("phase", phase_str(&status.phase).to_string());
    if let Some(exit) = status.exit_code {
        event = event.with_field("exit_code", exit.to_string());
    }
    if let Some(ref h) = status.pre_head {
        event = event.with_field("pre_head", h.clone());
    }
    if let Some(ref h) = status.post_head {
        event = event.with_field("post_head", h.clone());
    }
    if let Some(ref r) = status.preserved_ref {
        event = event.with_field("preserved_ref", r.clone());
    }
    if let Some(ref r) = status.preserved_stash_ref {
        event = event.with_field("preserved_stash_ref", r.clone());
    }
    if let Some(ref tail) = status.log_tail {
        event = event.with_field("log_tail", tail.clone());
    }
    if let Some(ref reason) = status.failure_reason {
        event = event.with_field("failure_reason", reason.clone());
    }
    publish(events, event);
}

pub(super) fn publish_ci_progress(
    events: &broadcast::Sender<JailgunEvent>,
    req: &DeployRequest,
    state: &CiState,
    attempt: u32,
) {
    let (label, severity) = match state {
        CiState::Pending { .. } => ("ci-pending", Severity::Info),
        CiState::Passed { .. } => ("ci-passed", Severity::Info),
        CiState::Failed { .. } => ("ci-failed", Severity::Error),
        CiState::Skipped { .. } => ("ci-skipped", Severity::Info),
        CiState::Unknown => ("ci-unknown", Severity::Info),
    };
    let mut event = JailgunEvent::new(req.run_id.clone(), EventKind::RemoteSafety, "ci state")
        .with_tab(req.tab_id)
        .with_severity(severity)
        .with_field("phase", label.to_string())
        .with_field("attempt", attempt.to_string());
    match state {
        CiState::Passed { run_url, .. } | CiState::Failed { run_url, .. } => {
            event = event.with_field("ci_run_url", run_url.clone());
        }
        _ => {}
    }
    publish(events, event);
}

pub(super) fn publish_finished(
    events: &broadcast::Sender<JailgunEvent>,
    req: &DeployRequest,
    receipt: &DeployReceipt,
) {
    let severity = match receipt.outcome {
        DeployOutcome::Succeeded | DeployOutcome::SucceededCiSkipped => Severity::Info,
        DeployOutcome::DryRunStaged => Severity::Info,
        _ => Severity::Error,
    };
    let mut event = JailgunEvent::new(
        req.run_id.clone(),
        EventKind::DeployFinished,
        "deploy finished",
    )
    .with_tab(req.tab_id)
    .with_severity(severity)
    .with_field("outcome", outcome_str(receipt.outcome).to_string())
    .with_field("remote_host", req.remote_host.clone())
    .with_field("remote_dir", req.remote_dir.clone())
    .with_field("remote_command", req.remote_command.clone())
    .with_field("local_sha256", receipt.local_sha256.clone())
    .with_field("remote_sha256", receipt.remote_sha256.clone());
    if let Some(ci_repo) = receipt.ci_repo.as_ref() {
        event = event.with_field("ci_repo", ci_repo.clone());
    }
    if let Some(exit) = receipt.final_status.exit_code {
        event = event.with_field("exit_code", exit.to_string());
    }
    if !receipt.log_tail.is_empty() {
        event = event.with_field("log_tail", receipt.log_tail.clone());
    }
    if let Some(ref head) = receipt.final_status.post_head {
        event = event.with_field("post_head", head.clone());
    }
    if let Some(ref reason) = receipt.final_status.failure_reason {
        event = event.with_field("failure_reason", reason.clone());
    }
    if let Some(ref preserved) = receipt.final_status.preserved_ref {
        event = event.with_field("preserved_ref", preserved.clone());
    }
    if let Some(ref preserved) = receipt.final_status.preserved_stash_ref {
        event = event.with_field("preserved_stash_ref", preserved.clone());
    }
    if let Some(ref path) = receipt.receipt_path {
        event = event.with_field("receipt_path", path.display().to_string());
    }
    if let Some(files) = receipt.final_status.files_changed {
        event = event.with_field("files_changed", files.to_string());
    }
    if let Some(adds) = receipt.final_status.additions {
        event = event.with_field("additions", adds.to_string());
    }
    if let Some(dels) = receipt.final_status.deletions {
        event = event.with_field("deletions", dels.to_string());
    }
    if !receipt.final_status.top_paths.is_empty() {
        event = event.with_field("top_paths", receipt.final_status.top_paths.join(","));
    }
    match &receipt.ci_state {
        CiState::Passed { run_url, .. } => {
            event = event
                .with_field("ci_state", "passed".to_string())
                .with_field("ci_run_url", run_url.clone());
        }
        CiState::Failed { run_url, .. } => {
            event = event
                .with_field("ci_state", "failed".to_string())
                .with_field("ci_run_url", run_url.clone());
        }
        CiState::Pending { .. } => {
            event = event.with_field("ci_state", "pending".to_string());
        }
        CiState::Skipped { reason } => {
            event = event
                .with_field("ci_state", "skipped".to_string())
                .with_field("ci_skip_reason", reason.clone());
        }
        CiState::Unknown => {}
    }
    publish(events, event);
}

pub(super) fn phase_str(phase: &JobPhase) -> &'static str {
    match phase {
        JobPhase::Queued => "queued",
        JobPhase::Uploading => "uploading",
        JobPhase::UploadVerified => "upload-verified",
        JobPhase::Running => "running",
        JobPhase::Unpacking => "unpacking",
        JobPhase::CommandRunning => "command-running",
        JobPhase::Done => "done",
        JobPhase::FailedPreserved => "failed-preserved",
        JobPhase::Failed => "failed",
        JobPhase::MissingStatus => "missing-status",
    }
}

fn outcome_str(outcome: DeployOutcome) -> &'static str {
    match outcome {
        DeployOutcome::Succeeded => "succeeded",
        DeployOutcome::SucceededCiFailed => "succeeded-ci-failed",
        DeployOutcome::SucceededCiSkipped => "succeeded-ci-skipped",
        DeployOutcome::FailedPreserved => "failed-preserved",
        DeployOutcome::FailedHard => "failed-hard",
        DeployOutcome::UploadShaMismatch => "upload-sha-mismatch",
        DeployOutcome::TimedOut => "timed-out",
        DeployOutcome::DryRunStaged => "dry-run-staged",
    }
}

//! Remote deploy orchestrator.
//!
//! `deploy_remote` is the free function that ties together the four trait
//! families (`RemoteUploadBackend`, `RemoteJobBackend`, `CiTracker`,
//! `JsonReceiptWriter`) plus the broadcast event bus. Production wiring uses
//! the SSH backends in `shell.rs`; tests use the fakes in `#[cfg(test)]` here
//! and (for cross-crate use) the `fake-backends` Cargo feature module
//! `crate::fake`.

use std::path::PathBuf;

use async_trait::async_trait;
use jailgun_core::{CleanupPolicy, EventKind, JailgunEvent, Severity};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::sync::broadcast;

use crate::{
    ci::{CiState, CiTracker},
    job::{JobHandle, JobPhase, JobSpec, JobStatus, RemoteJobBackend},
    upload::RemoteUploadBackend,
    util::truncate_log_tail,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeployRequest {
    pub run_id: String,
    pub tab_id: u16,
    pub remote_host: String,
    pub remote_dir: String,
    pub remote_command: String,
    pub remote_archive_basename: String,
    pub local_archive_path: PathBuf,
    pub strip_components: u16,
    pub cleanup_policy: CleanupPolicy,
    pub receipt_dir: PathBuf,
    pub status_poll_seconds: u16,
    pub status_max_minutes: u16,
    pub ci_tracker_enabled: bool,
    pub ci_branch: String,
    pub ci_max_attempts: u32,
    pub ci_poll_seconds: u16,
    pub stash_on_failure: bool,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeployReceipt {
    pub run_id: String,
    pub tab_id: u16,
    pub remote_host: String,
    pub remote_dir: String,
    pub started_at: String,
    pub finished_at: String,
    pub local_archive_path: PathBuf,
    pub local_sha256: String,
    pub remote_sha256: String,
    pub remote_archive_path: String,
    pub job_handle: JobHandle,
    pub final_status: JobStatus,
    pub ci_state: CiState,
    pub log_tail: String,
    pub outcome: DeployOutcome,
    pub receipt_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DeployOutcome {
    Succeeded,
    SucceededCiFailed,
    SucceededCiSkipped,
    FailedPreserved,
    FailedHard,
    UploadShaMismatch,
    TimedOut,
    DryRunStaged,
}

#[derive(Debug, Error)]
pub enum DeployError {
    #[error("ssh transport failure: {0}")]
    Ssh(String),
    #[error("scp transport failure: {0}")]
    Scp(String),
    #[error("remote sha256 mismatch: local={local} remote={remote}")]
    ShaMismatch { local: String, remote: String },
    #[error("remote dir preparation failed: {0}")]
    RemoteDirPrep(String),
    #[error("launcher install failed: {0}")]
    LauncherInstall(String),
    #[error("launcher start failed: {0}")]
    LauncherStart(String),
    #[error("status fetch failed: {0}")]
    StatusFetch(String),
    #[error("status parse failed: {0}")]
    StatusParse(String),
    #[error("log fetch failed: {0}")]
    LogFetch(String),
    #[error("deploy timed out after {0} minute(s)")]
    Timeout(u16),
    #[error("CI tracker error: {0}")]
    CiTracker(String),
    #[error("receipt write failed: {0}")]
    Receipt(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("cleanup error: {0}")]
    Cleanup(#[from] crate::cleanup::CleanupError),
    #[error("sha256 error: {0}")]
    Sha256(String),
}

impl From<jailgun_core::receipt::ReceiptError> for DeployError {
    fn from(error: jailgun_core::receipt::ReceiptError) -> Self {
        DeployError::Sha256(error.to_string())
    }
}

/// Sink for the final receipt JSON. `SshRemoteUpload` is not the right home
/// because receipts are written locally next to the run artifacts.
#[async_trait]
pub trait JsonReceiptWriter {
    async fn write_receipt(&mut self, receipt: &DeployReceipt) -> Result<PathBuf, DeployError>;
}

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

    publish(
        events,
        JailgunEvent::new(req.run_id.clone(), EventKind::DeployQueued, "deploy queued")
            .with_tab(req.tab_id)
            .with_field("local_sha256", local_sha256.clone())
            .with_field(
                "local_archive_path",
                req.local_archive_path.display().to_string(),
            )
            .with_field("remote_host", req.remote_host.clone())
            .with_field("remote_dir", req.remote_dir.clone()),
    );

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
    let log_tail = job.fetch_log_tail(&handle, 40).await.unwrap_or_default();
    let log_tail = truncate_log_tail(&log_tail, 20, 4096);

    let mut status = final_status;
    status.log_tail = Some(log_tail.clone());

    let outcome_pre_ci = match status.phase {
        JobPhase::Done => DeployOutcome::Succeeded,
        JobPhase::FailedPreserved => DeployOutcome::FailedPreserved,
        JobPhase::Failed => DeployOutcome::FailedHard,
        _ => DeployOutcome::TimedOut,
    };

    let ci_state = if matches!(outcome_pre_ci, DeployOutcome::Succeeded)
        && req.ci_tracker_enabled
        && status
            .post_head
            .as_ref()
            .map(|h| !h.is_empty())
            .unwrap_or(false)
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

#[allow(clippy::too_many_arguments)]
fn build_receipt(
    req: &DeployRequest,
    started_at: String,
    finished_at: String,
    local_sha256: String,
    remote_sha256: String,
    remote_archive_path: String,
    job_handle: JobHandle,
    final_status: JobStatus,
    ci_state: CiState,
    log_tail: String,
    outcome: DeployOutcome,
) -> DeployReceipt {
    DeployReceipt {
        run_id: req.run_id.clone(),
        tab_id: req.tab_id,
        remote_host: req.remote_host.clone(),
        remote_dir: req.remote_dir.clone(),
        started_at,
        finished_at,
        local_archive_path: req.local_archive_path.clone(),
        local_sha256,
        remote_sha256,
        remote_archive_path,
        job_handle,
        final_status,
        ci_state,
        log_tail,
        outcome,
        receipt_path: None,
    }
}

async fn poll_until_terminal<J>(
    job: &mut J,
    handle: &JobHandle,
    req: &DeployRequest,
    events: &broadcast::Sender<JailgunEvent>,
) -> Result<JobStatus, DeployError>
where
    J: RemoteJobBackend + Send,
{
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs(req.status_max_minutes as u64 * 60);
    let poll_interval = std::time::Duration::from_secs(req.status_poll_seconds as u64);
    let mut consecutive_errors: u8 = 0;
    let mut last_seen: JobStatus = JobStatus::default();
    loop {
        if std::time::Instant::now() >= deadline {
            publish(
                events,
                JailgunEvent::new(
                    req.run_id.clone(),
                    EventKind::DeployFinished,
                    "deploy timeout",
                )
                .with_tab(req.tab_id)
                .with_severity(Severity::Error)
                .with_field("outcome", "timed-out")
                .with_field("reason", "status_max_minutes exceeded"),
            );
            return Err(DeployError::Timeout(req.status_max_minutes));
        }
        tokio::time::sleep(poll_interval).await;
        match job.fetch_status(handle).await {
            Ok(status) => {
                consecutive_errors = 0;
                publish_status_progress(events, req, &status);
                if status.phase.is_terminal() {
                    return Ok(status);
                }
                last_seen = status;
            }
            Err(error) => {
                consecutive_errors = consecutive_errors.saturating_add(1);
                tracing::warn!(?error, attempts = consecutive_errors, "status fetch failed");
                publish(
                    events,
                    JailgunEvent::new(
                        req.run_id.clone(),
                        EventKind::RemoteSafety,
                        "status fetch error",
                    )
                    .with_tab(req.tab_id)
                    .with_severity(Severity::Warn)
                    .with_field("phase", "status-fetch-error")
                    .with_field("attempts", consecutive_errors.to_string())
                    .with_field("last_phase", phase_str(&last_seen.phase).to_string()),
                );
                if consecutive_errors >= 5 {
                    return Err(error);
                }
            }
        }
    }
}

async fn track_ci<C: CiTracker + Send>(
    ci: &mut C,
    status: &JobStatus,
    req: &DeployRequest,
    events: &broadcast::Sender<JailgunEvent>,
) -> CiState {
    let Some(commit_sha) = status.post_head.as_ref() else {
        return CiState::Unknown;
    };
    let interval = std::time::Duration::from_secs(req.ci_poll_seconds as u64);
    let mut last_observed = CiState::Unknown;
    for attempt in 1..=req.ci_max_attempts {
        match ci.check(commit_sha, &req.ci_branch).await {
            Ok(state) => {
                publish_ci_progress(events, req, &state, attempt);
                if state.is_terminal() {
                    if let CiState::Failed { run_id, .. } = &state {
                        let excerpt = ci.capture_failure_log(run_id, 16 * 1024).await.ok();
                        let mut final_state = state.clone();
                        if let CiState::Failed { log_excerpt, .. } = &mut final_state {
                            *log_excerpt = excerpt;
                        }
                        return final_state;
                    }
                    return state;
                }
                last_observed = state;
            }
            Err(error) => {
                tracing::warn!(?error, attempt, "CI check failed");
                publish(
                    events,
                    JailgunEvent::new(
                        req.run_id.clone(),
                        EventKind::RemoteSafety,
                        "ci tracker transient error",
                    )
                    .with_tab(req.tab_id)
                    .with_severity(Severity::Warn)
                    .with_field("phase", "ci-error")
                    .with_field("attempt", attempt.to_string()),
                );
            }
        }
        tokio::time::sleep(interval).await;
    }
    last_observed
}

fn publish(events: &broadcast::Sender<JailgunEvent>, event: JailgunEvent) {
    let _ = events.send(event);
}

fn publish_error<E: std::fmt::Display>(
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

fn publish_status_progress(
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

fn publish_ci_progress(
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

fn publish_finished(
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
    .with_field("local_sha256", receipt.local_sha256.clone())
    .with_field("remote_sha256", receipt.remote_sha256.clone());
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
    publish(events, event);
}

fn timestamp_now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn phase_str(phase: &JobPhase) -> &'static str {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, path::Path};

    use jailgun_core::CleanupPolicy;
    use tempfile::TempDir;
    use tokio::sync::broadcast;

    struct FakeUpload {
        ensure_calls: usize,
        upload_calls: usize,
        sha_responses: VecDeque<String>,
        remove_calls: usize,
    }
    impl FakeUpload {
        fn new(shas: Vec<String>) -> Self {
            Self {
                ensure_calls: 0,
                upload_calls: 0,
                sha_responses: shas.into(),
                remove_calls: 0,
            }
        }
    }
    #[async_trait]
    impl RemoteUploadBackend for FakeUpload {
        async fn ensure_remote_dir(&mut self, _remote_dir: &str) -> Result<(), DeployError> {
            self.ensure_calls += 1;
            Ok(())
        }
        async fn upload_archive(
            &mut self,
            _local: &Path,
            _remote: &str,
        ) -> Result<(), DeployError> {
            self.upload_calls += 1;
            Ok(())
        }
        async fn remote_sha256(&mut self, _remote: &str) -> Result<String, DeployError> {
            self.sha_responses
                .pop_front()
                .ok_or_else(|| DeployError::Ssh("no scripted sha".into()))
        }
        async fn remove_remote_file(&mut self, _remote: &str) -> Result<(), DeployError> {
            self.remove_calls += 1;
            Ok(())
        }
    }

    struct FakeJob {
        install_called: bool,
        start_called: bool,
        statuses: VecDeque<JobStatus>,
        log: String,
        last_spec: Option<JobSpec>,
    }
    impl FakeJob {
        fn new(statuses: Vec<JobStatus>) -> Self {
            Self {
                install_called: false,
                start_called: false,
                statuses: statuses.into(),
                log: String::from("ok"),
                last_spec: None,
            }
        }
    }
    #[async_trait]
    impl RemoteJobBackend for FakeJob {
        async fn install_launcher(&mut self, spec: &JobSpec) -> Result<JobHandle, DeployError> {
            self.install_called = true;
            self.last_spec = Some(spec.clone());
            Ok(JobHandle {
                job_id: format!("{}-tab-{:02}", spec.run_id, spec.tab_id),
                launcher_dir: format!("$HOME/.jailgun/runs/{}-tab-{:02}", spec.run_id, spec.tab_id),
                launcher_path: "launch.sh".into(),
                status_path: "status.json".into(),
                log_path: "launch.log".into(),
                failure_marker_path: "deploy.failed".into(),
            })
        }
        async fn start_job(
            &mut self,
            _spec: &JobSpec,
            _handle: &JobHandle,
        ) -> Result<(), DeployError> {
            self.start_called = true;
            Ok(())
        }
        async fn fetch_status(&mut self, _handle: &JobHandle) -> Result<JobStatus, DeployError> {
            self.statuses
                .pop_front()
                .ok_or_else(|| DeployError::StatusFetch("no scripted status".into()))
        }
        async fn fetch_log_tail(
            &mut self,
            _handle: &JobHandle,
            _last_n_lines: usize,
        ) -> Result<String, DeployError> {
            Ok(self.log.clone())
        }
    }

    struct FakeCi(VecDeque<CiState>);
    #[async_trait]
    impl CiTracker for FakeCi {
        async fn check(&mut self, _sha: &str, _branch: &str) -> Result<CiState, DeployError> {
            self.0
                .pop_front()
                .ok_or_else(|| DeployError::CiTracker("no scripted state".into()))
        }
        async fn capture_failure_log(
            &mut self,
            _run_id: &str,
            _max: usize,
        ) -> Result<String, DeployError> {
            Ok("--- failed log excerpt ---".into())
        }
    }

    struct FakeWriter {
        receipts: Vec<DeployReceipt>,
    }
    #[async_trait]
    impl JsonReceiptWriter for FakeWriter {
        async fn write_receipt(&mut self, receipt: &DeployReceipt) -> Result<PathBuf, DeployError> {
            self.receipts.push(receipt.clone());
            Ok(PathBuf::from(format!(
                "receipts/{}-tab-{:02}.json",
                receipt.run_id, receipt.tab_id
            )))
        }
    }

    fn fake_request(archive: PathBuf) -> DeployRequest {
        DeployRequest {
            run_id: "run-test".into(),
            tab_id: 1,
            remote_host: "example-host".into(),
            remote_dir: "/srv/project".into(),
            remote_command: "true".into(),
            remote_archive_basename: "x.tar.gz".into(),
            local_archive_path: archive,
            strip_components: 1,
            cleanup_policy: CleanupPolicy::PreserveReset,
            receipt_dir: PathBuf::from("/tmp/receipts"),
            status_poll_seconds: 1,
            status_max_minutes: 1,
            ci_tracker_enabled: false,
            ci_branch: "main".into(),
            ci_max_attempts: 3,
            ci_poll_seconds: 1,
            stash_on_failure: true,
            dry_run: false,
        }
    }

    async fn make_archive() -> (TempDir, PathBuf, String) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("x.tar.gz");
        tokio::fs::write(&path, b"hello world").await.unwrap();
        let sha = jailgun_core::sha256_file(&path).unwrap();
        (dir, path, sha)
    }

    #[tokio::test]
    async fn dry_run_emits_dry_run_staged_outcome() {
        let (_dir, archive, sha) = make_archive().await;
        let mut upload = FakeUpload::new(vec![sha.clone()]);
        let mut job = FakeJob::new(vec![]);
        let mut ci = FakeCi(vec![].into());
        let mut writer = FakeWriter { receipts: vec![] };
        let (tx, _rx) = broadcast::channel(64);
        let mut req = fake_request(archive);
        req.dry_run = true;
        let receipt = deploy_remote(&mut upload, &mut job, &mut ci, &mut writer, req, &tx)
            .await
            .expect("dry run ok");
        assert_eq!(receipt.outcome, DeployOutcome::DryRunStaged);
        assert!(job.install_called);
        assert!(!job.start_called);
        assert_eq!(writer.receipts.len(), 1);
    }

    #[tokio::test]
    async fn sha_mismatch_returns_err_and_removes_remote_file() {
        let (_dir, archive, _sha) = make_archive().await;
        let mut upload = FakeUpload::new(vec!["different".repeat(8)]);
        let mut job = FakeJob::new(vec![]);
        let mut ci = FakeCi(vec![].into());
        let mut writer = FakeWriter { receipts: vec![] };
        let (tx, _rx) = broadcast::channel(64);
        let err = deploy_remote(
            &mut upload,
            &mut job,
            &mut ci,
            &mut writer,
            fake_request(archive),
            &tx,
        )
        .await
        .expect_err("mismatch");
        assert!(matches!(err, DeployError::ShaMismatch { .. }));
        assert_eq!(upload.remove_calls, 1);
        assert!(writer.receipts.is_empty());
    }

    #[tokio::test]
    async fn success_path_records_succeeded_ci_skipped() {
        let (_dir, archive, sha) = make_archive().await;
        let mut upload = FakeUpload::new(vec![sha.clone()]);
        let mut job = FakeJob::new(vec![JobStatus {
            phase: JobPhase::Done,
            exit_code: Some(0),
            pre_head: Some("abc".into()),
            post_head: Some("abc".into()),
            ..Default::default()
        }]);
        let mut ci = FakeCi(vec![].into());
        let mut writer = FakeWriter { receipts: vec![] };
        let (tx, _rx) = broadcast::channel(64);
        let mut req = fake_request(archive);
        req.status_poll_seconds = 1;
        let receipt = deploy_remote(&mut upload, &mut job, &mut ci, &mut writer, req, &tx)
            .await
            .expect("success");
        assert_eq!(receipt.outcome, DeployOutcome::SucceededCiSkipped);
    }

    #[tokio::test]
    async fn failed_preserved_records_outcome_with_preserved_refs() {
        let (_dir, archive, sha) = make_archive().await;
        let mut upload = FakeUpload::new(vec![sha.clone()]);
        let mut job = FakeJob::new(vec![JobStatus {
            phase: JobPhase::FailedPreserved,
            exit_code: Some(23),
            pre_head: Some("abc".into()),
            post_head: Some("def".into()),
            preserved_ref: Some("jailgun-failed/run-test-tab-01".into()),
            preserved_stash_ref: Some("jailgun-failed/run-test-tab-01-stash".into()),
            failure_reason: Some("remote-command-failed".into()),
            reset_ok: Some(true),
            ..Default::default()
        }]);
        let mut ci = FakeCi(vec![].into());
        let mut writer = FakeWriter { receipts: vec![] };
        let (tx, _rx) = broadcast::channel(64);
        let receipt = deploy_remote(
            &mut upload,
            &mut job,
            &mut ci,
            &mut writer,
            fake_request(archive),
            &tx,
        )
        .await
        .expect("preserved outcome is not an Err");
        assert_eq!(receipt.outcome, DeployOutcome::FailedPreserved);
        assert_eq!(
            receipt.final_status.preserved_ref.as_deref(),
            Some("jailgun-failed/run-test-tab-01")
        );
        assert_eq!(
            receipt.final_status.preserved_stash_ref.as_deref(),
            Some("jailgun-failed/run-test-tab-01-stash")
        );
    }
}

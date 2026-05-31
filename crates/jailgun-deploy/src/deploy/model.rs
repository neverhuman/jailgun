use std::path::PathBuf;

use async_trait::async_trait;
use jailgun_core::{AgentError, AgentErrorExt, CleanupPolicy};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ci::CiState,
    job::{JobHandle, JobStatus},
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

impl AgentErrorExt for DeployError {
    fn agent_error(&self) -> AgentError {
        let code = match self {
            DeployError::Ssh(_) => "deploy-ssh",
            DeployError::Scp(_) => "deploy-scp",
            DeployError::ShaMismatch { .. } => "deploy-sha-mismatch",
            DeployError::RemoteDirPrep(_) => "deploy-remote-dir",
            DeployError::LauncherInstall(_) => "deploy-launcher-install",
            DeployError::LauncherStart(_) => "deploy-launcher-start",
            DeployError::StatusFetch(_) => "deploy-status-fetch",
            DeployError::StatusParse(_) => "deploy-status-parse",
            DeployError::LogFetch(_) => "deploy-log-fetch",
            DeployError::Timeout(_) => "deploy-timeout",
            DeployError::CiTracker(_) => "deploy-ci-tracker",
            DeployError::Receipt(_) => "deploy-receipt",
            DeployError::Io(_) => "deploy-io",
            DeployError::Serde(_) => "deploy-serde",
            DeployError::Cleanup(_) => "deploy-cleanup",
            DeployError::Sha256(_) => "deploy-sha256",
        };
        AgentError::new(
            code,
            "stage source archive and execute remote deploy",
            self.to_string(),
            vec![
                "rerun fake deploy tests before touching real remotes",
                "check remote cleanup receipt and launcher status JSON",
                "keep SSH/SCP access behind Remote* trait boundaries",
            ],
            "docs/boundaries.md",
            "rerun `cargo test -p jailgun-deploy --features fake-backends`",
        )
    }
}

/// Sink for the final receipt JSON. `SshRemoteUpload` is not the right home
/// because receipts are written locally next to the run artifacts.
#[async_trait]
pub trait JsonReceiptWriter {
    async fn write_receipt(&mut self, receipt: &DeployReceipt) -> Result<PathBuf, DeployError>;
}

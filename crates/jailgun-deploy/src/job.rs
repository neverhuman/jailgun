//! Remote job backend trait + status payload types.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::deploy::DeployError;

/// Inputs the launcher needs to know in order to extract and execute the
/// archive on the remote host.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobSpec {
    pub run_id: String,
    pub tab_id: u16,
    pub remote_dir: String,
    pub remote_archive_path: String,
    pub remote_command: String,
    pub strip_components: u16,
    pub local_sha256: String,
    pub remote_sha256: String,
    pub stash_on_failure: bool,
}

/// Stable on-remote paths produced by `install_launcher`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobHandle {
    pub job_id: String,
    pub launcher_dir: String,
    pub launcher_path: String,
    pub status_path: String,
    pub log_path: String,
    pub failure_marker_path: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum JobPhase {
    Queued,
    Uploading,
    UploadVerified,
    Running,
    Unpacking,
    CommandRunning,
    Done,
    FailedPreserved,
    Failed,
    #[default]
    MissingStatus,
}

impl JobPhase {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            JobPhase::Done | JobPhase::Failed | JobPhase::FailedPreserved
        )
    }

    pub fn is_success(self) -> bool {
        matches!(self, JobPhase::Done)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct JobStatus {
    pub phase: JobPhase,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub pre_head: Option<String>,
    #[serde(default)]
    pub post_head: Option<String>,
    #[serde(default)]
    pub preserved_ref: Option<String>,
    #[serde(default)]
    pub preserved_sha: Option<String>,
    #[serde(default)]
    pub preserved_stash: Option<String>,
    #[serde(default)]
    pub preserved_stash_ref: Option<String>,
    #[serde(default)]
    pub preserved_patch_path: Option<String>,
    #[serde(default)]
    pub reset_to: Option<String>,
    #[serde(default)]
    pub reset_ok: Option<bool>,
    #[serde(default)]
    pub failure_reason: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub finished_at: Option<String>,
    #[serde(default)]
    pub failed_at: Option<String>,
    #[serde(default)]
    pub log_tail: Option<String>,
    /// `git diff --shortstat` output parsed into structured numbers; populated
    /// by the launcher when the run produced a new commit.
    #[serde(default)]
    pub files_changed: Option<u32>,
    #[serde(default)]
    pub additions: Option<u32>,
    #[serde(default)]
    pub deletions: Option<u32>,
    #[serde(default)]
    pub top_paths: Vec<String>,
    #[serde(default)]
    pub changed_paths: Vec<String>,
    #[serde(default)]
    pub pre_status: Vec<String>,
    #[serde(default)]
    pub post_status: Vec<String>,
    #[serde(default)]
    pub shortstat: Option<String>,
    /// Preserved verbatim so forensics work even when our typed fields evolve
    /// out of sync with the launcher's emitted shape.
    #[serde(default)]
    pub raw: serde_json::Value,
}

#[async_trait]
pub trait RemoteJobBackend {
    async fn install_launcher(&mut self, spec: &JobSpec) -> Result<JobHandle, DeployError>;
    async fn start_job(&mut self, spec: &JobSpec, handle: &JobHandle) -> Result<(), DeployError>;
    async fn fetch_status(&mut self, handle: &JobHandle) -> Result<JobStatus, DeployError>;
    async fn fetch_log_tail(
        &mut self,
        handle: &JobHandle,
        last_n_lines: usize,
    ) -> Result<String, DeployError>;
}

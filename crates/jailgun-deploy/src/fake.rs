//! Env-driven fake backends for CI end-to-end testing.
//!
//! Activated by `JAILGUN_FAKE_REMOTE=1`. The fake outcome is selected via
//! `JAILGUN_FAKE_REMOTE_RESULT`:
//!
//! - `success` (default): cleanup is already-synced, upload sha matches,
//!   remote job phase=done, CI=skipped.
//! - `sha-mismatch`: remote sha returns a deterministic wrong value.
//! - `command-fail`: launcher phase=failed-preserved with preserved_ref +
//!   preserved_stash_ref set.
//! - `ci-fail`: deploy succeeds but CI tracker reports Failed.
//! - `cleanup-divergent`: cleanup sees a divergent HEAD; preserve-reset path
//!   exercised.
//!
//! The trait impls live behind the `fake-backends` Cargo feature so they
//! never ship in production binaries unless the feature is explicitly
//! enabled.

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::Mutex,
};

use async_trait::async_trait;

use crate::{
    ci::{CiState, CiTracker},
    cleanup::{CleanupError, CleanupReceipt, RemoteGitBackend, RemoteSnapshot},
    deploy::{DeployError, DeployReceipt, JsonReceiptWriter},
    job::{JobHandle, JobPhase, JobSpec, JobStatus, RemoteJobBackend},
    upload::RemoteUploadBackend,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FakeOutcome {
    Success,
    ShaMismatch,
    CommandFail,
    CiFail,
    CleanupDivergent,
}

impl FakeOutcome {
    pub fn from_env() -> Self {
        match std::env::var("JAILGUN_FAKE_REMOTE_RESULT").ok().as_deref() {
            Some("sha-mismatch") => FakeOutcome::ShaMismatch,
            Some("command-fail") => FakeOutcome::CommandFail,
            Some("ci-fail") => FakeOutcome::CiFail,
            Some("cleanup-divergent") => FakeOutcome::CleanupDivergent,
            _ => FakeOutcome::Success,
        }
    }
}

#[derive(Debug)]
pub struct FakeBus {
    pub outcome: FakeOutcome,
}

impl FakeBus {
    pub fn from_env() -> Self {
        Self {
            outcome: FakeOutcome::from_env(),
        }
    }
}

pub struct FakeRemoteGit {
    outcome: FakeOutcome,
    snapshots: Mutex<VecDeque<RemoteSnapshot>>,
}

impl FakeRemoteGit {
    pub fn new(outcome: FakeOutcome) -> Self {
        let snapshots: VecDeque<RemoteSnapshot> = match outcome {
            FakeOutcome::CleanupDivergent => VecDeque::from(vec![
                RemoteSnapshot::clean("head-fake-a", "head-fake-b"),
                RemoteSnapshot::clean("head-fake-a", "head-fake-b"),
                RemoteSnapshot::clean("head-fake-b", "head-fake-b"),
            ]),
            _ => VecDeque::from(vec![RemoteSnapshot::clean("head-fake-a", "head-fake-a")]),
        };
        Self {
            outcome,
            snapshots: Mutex::new(snapshots),
        }
    }
}

#[async_trait]
impl RemoteGitBackend for FakeRemoteGit {
    async fn snapshot(&mut self, _remote_dir: &str) -> Result<RemoteSnapshot, CleanupError> {
        let mut guard = self
            .snapshots
            .lock()
            .map_err(|_| CleanupError::Backend("poisoned snapshot lock".into()))?;
        guard
            .pop_front()
            .or_else(|| guard.back().cloned())
            .ok_or_else(|| CleanupError::Backend("no fake snapshot".into()))
    }

    async fn fetch_origin(&mut self, _remote_dir: &str) -> Result<(), CleanupError> {
        Ok(())
    }

    async fn create_ref(
        &mut self,
        _remote_dir: &str,
        _ref_name: &str,
        _sha: &str,
    ) -> Result<(), CleanupError> {
        Ok(())
    }

    async fn write_receipt(&mut self, receipt: &CleanupReceipt) -> Result<PathBuf, CleanupError> {
        Ok(receipt
            .receipt_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("fake-cleanup-receipt.json")))
    }

    async fn reset_hard(&mut self, _remote_dir: &str, _target: &str) -> Result<(), CleanupError> {
        if matches!(self.outcome, FakeOutcome::CleanupDivergent) {
            // After reset, advance to clean state.
            if let Ok(mut guard) = self.snapshots.lock() {
                guard.clear();
                guard.push_back(RemoteSnapshot::clean("head-fake-b", "head-fake-b"));
            }
        }
        Ok(())
    }
}

pub struct FakeRemoteUpload {
    outcome: FakeOutcome,
}

impl FakeRemoteUpload {
    pub fn new(outcome: FakeOutcome) -> Self {
        Self { outcome }
    }
}

#[async_trait]
impl RemoteUploadBackend for FakeRemoteUpload {
    async fn ensure_remote_dir(&mut self, _remote_dir: &str) -> Result<(), DeployError> {
        Ok(())
    }
    async fn upload_archive(&mut self, _local: &Path, _remote: &str) -> Result<(), DeployError> {
        Ok(())
    }
    async fn remote_sha256(&mut self, _remote: &str) -> Result<String, DeployError> {
        match self.outcome {
            FakeOutcome::ShaMismatch => Ok("0".repeat(64)),
            _ => Ok(std::env::var("JAILGUN_FAKE_LOCAL_SHA").unwrap_or_else(|_| "a".repeat(64))),
        }
    }
    async fn remove_remote_file(&mut self, _remote: &str) -> Result<(), DeployError> {
        Ok(())
    }
}

pub struct FakeRemoteJob {
    outcome: FakeOutcome,
}

impl FakeRemoteJob {
    pub fn new(outcome: FakeOutcome) -> Self {
        Self { outcome }
    }
}

#[async_trait]
impl RemoteJobBackend for FakeRemoteJob {
    async fn install_launcher(&mut self, spec: &JobSpec) -> Result<JobHandle, DeployError> {
        let job_id = format!("{}-tab-{:02}", spec.run_id, spec.tab_id);
        Ok(JobHandle {
            job_id: job_id.clone(),
            launcher_dir: format!("/tmp/jailgun-runs/{job_id}"),
            launcher_path: format!("/tmp/jailgun-runs/{job_id}/launch.sh"),
            status_path: format!("/tmp/jailgun-runs/{job_id}/status.json"),
            log_path: format!("/tmp/jailgun-runs/{job_id}/launch.log"),
            failure_marker_path: format!("/tmp/jailgun-runs/{job_id}/deploy.failed"),
        })
    }
    async fn start_job(&mut self, _spec: &JobSpec, _handle: &JobHandle) -> Result<(), DeployError> {
        Ok(())
    }
    async fn fetch_status(&mut self, _handle: &JobHandle) -> Result<JobStatus, DeployError> {
        match self.outcome {
            FakeOutcome::CommandFail => Ok(JobStatus {
                phase: JobPhase::FailedPreserved,
                exit_code: Some(23),
                pre_head: Some("head-fake-a".into()),
                post_head: Some("head-fake-b".into()),
                preserved_ref: Some("jailgun-failed/fake-tab-01".into()),
                preserved_stash_ref: Some("jailgun-failed/fake-tab-01-stash".into()),
                failure_reason: Some("remote-command-failed".into()),
                reset_ok: Some(true),
                ..Default::default()
            }),
            _ => Ok(JobStatus {
                phase: JobPhase::Done,
                exit_code: Some(0),
                pre_head: Some("head-fake-a".into()),
                post_head: Some("head-fake-c".into()),
                files_changed: Some(3),
                additions: Some(15),
                deletions: Some(2),
                top_paths: vec!["README.md".into(), "src/lib.rs".into()],
                ..Default::default()
            }),
        }
    }
    async fn fetch_log_tail(
        &mut self,
        _handle: &JobHandle,
        _n: usize,
    ) -> Result<String, DeployError> {
        Ok("fake remote log\nbuild ok\n".into())
    }
}

pub struct FakeCiTracker {
    outcome: FakeOutcome,
}

impl FakeCiTracker {
    pub fn new(outcome: FakeOutcome) -> Self {
        Self { outcome }
    }
}

#[async_trait]
impl CiTracker for FakeCiTracker {
    async fn check(&mut self, _commit: &str, _branch: &str) -> Result<CiState, DeployError> {
        match self.outcome {
            FakeOutcome::CiFail => Ok(CiState::Failed {
                run_id: "100".into(),
                run_url: "https://example.invalid/actions/runs/100".into(),
                conclusion: "failure".into(),
                log_excerpt: None,
            }),
            _ => Ok(CiState::Passed {
                run_id: "101".into(),
                run_url: "https://example.invalid/actions/runs/101".into(),
                conclusion: "success".into(),
            }),
        }
    }
    async fn capture_failure_log(
        &mut self,
        _run_id: &str,
        _max: usize,
    ) -> Result<String, DeployError> {
        Ok("--- fake failure log excerpt ---".into())
    }
}

pub struct FakeReceiptWriter {
    pub root: PathBuf,
}

impl FakeReceiptWriter {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl JsonReceiptWriter for FakeReceiptWriter {
    async fn write_receipt(&mut self, receipt: &DeployReceipt) -> Result<PathBuf, DeployError> {
        let dir = self.root.join(&receipt.run_id);
        tokio::fs::create_dir_all(&dir).await?;
        let path = dir.join(format!(
            "{}-tab-{:02}-deploy.json",
            receipt.run_id, receipt.tab_id
        ));
        let bytes = serde_json::to_vec_pretty(receipt)?;
        tokio::fs::write(&path, bytes).await?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_from_env_defaults_to_success() {
        // SAFETY: tests run single-threaded for this env-touching block.
        std::env::remove_var("JAILGUN_FAKE_REMOTE_RESULT");
        assert_eq!(FakeOutcome::from_env(), FakeOutcome::Success);
        std::env::set_var("JAILGUN_FAKE_REMOTE_RESULT", "command-fail");
        assert_eq!(FakeOutcome::from_env(), FakeOutcome::CommandFail);
        std::env::set_var("JAILGUN_FAKE_REMOTE_RESULT", "ci-fail");
        assert_eq!(FakeOutcome::from_env(), FakeOutcome::CiFail);
        std::env::remove_var("JAILGUN_FAKE_REMOTE_RESULT");
    }
}

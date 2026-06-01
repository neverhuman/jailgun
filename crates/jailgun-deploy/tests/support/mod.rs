#![allow(dead_code)]

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use jailgun_core::{CleanupPolicy, JailgunEvent};
use jailgun_deploy::{
    CiState, CiTracker, CleanupError, CleanupReceipt, CleanupRequest, DeployError, DeployReceipt,
    DeployRequest, JobHandle, JobSpec, JobStatus, JsonReceiptWriter, RemoteGitBackend,
    RemoteJobBackend, RemoteSnapshot, RemoteUploadBackend,
};
use tempfile::TempDir;
use tokio::sync::broadcast;

pub struct FakeCleanupRemote {
    snapshots: VecDeque<RemoteSnapshot>,
    pub refs: Vec<(String, String)>,
    pub reset_targets: Vec<String>,
    pub receipt_writes: usize,
    pub fail_ref: bool,
    pub fail_receipt: bool,
}

impl FakeCleanupRemote {
    pub fn new(snapshots: Vec<RemoteSnapshot>) -> Self {
        Self {
            snapshots: snapshots.into(),
            refs: Vec::new(),
            reset_targets: Vec::new(),
            receipt_writes: 0,
            fail_ref: false,
            fail_receipt: false,
        }
    }
}

#[async_trait]
impl RemoteGitBackend for FakeCleanupRemote {
    async fn snapshot(&mut self, _remote_dir: &str) -> Result<RemoteSnapshot, CleanupError> {
        self.snapshots
            .pop_front()
            .or_else(|| self.snapshots.back().cloned())
            .ok_or_else(|| CleanupError::Backend("no fake snapshot".into()))
    }

    async fn fetch_origin(&mut self, _remote_dir: &str) -> Result<(), CleanupError> {
        Ok(())
    }

    async fn create_ref(
        &mut self,
        _remote_dir: &str,
        ref_name: &str,
        sha: &str,
    ) -> Result<(), CleanupError> {
        if self.fail_ref {
            return Err(CleanupError::Backend("ref rejected".into()));
        }
        self.refs.push((ref_name.into(), sha.into()));
        Ok(())
    }

    async fn write_receipt(&mut self, receipt: &CleanupReceipt) -> Result<PathBuf, CleanupError> {
        if self.fail_receipt {
            return Err(CleanupError::Backend("disk full".into()));
        }
        self.receipt_writes += 1;
        Ok(receipt
            .receipt_path
            .clone()
            .unwrap_or_else(|| PathBuf::from(format!("receipt-{}.json", self.receipt_writes))))
    }

    async fn reset_hard(&mut self, _remote_dir: &str, target: &str) -> Result<(), CleanupError> {
        self.reset_targets.push(target.into());
        Ok(())
    }
}

pub fn cleanup_request(policy: CleanupPolicy) -> CleanupRequest {
    CleanupRequest {
        run_id: "run-one".into(),
        tab_id: Some(3),
        remote_host: "example-host".into(),
        remote_dir: "/srv/project".into(),
        policy,
        receipt_dir: PathBuf::from("receipts"),
    }
}

pub struct FakeUpload {
    pub ensure_calls: usize,
    pub upload_calls: usize,
    sha_responses: VecDeque<String>,
    pub remove_calls: usize,
}

impl FakeUpload {
    pub fn new(shas: Vec<String>) -> Self {
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

    async fn upload_archive(&mut self, _local: &Path, _remote: &str) -> Result<(), DeployError> {
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

pub struct FakeJob {
    pub install_called: bool,
    pub start_called: bool,
    statuses: VecDeque<JobStatus>,
    log: String,
    pub last_spec: Option<JobSpec>,
}

impl FakeJob {
    pub fn new(statuses: Vec<JobStatus>) -> Self {
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

    async fn start_job(&mut self, _spec: &JobSpec, _handle: &JobHandle) -> Result<(), DeployError> {
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

pub struct FakeCi(pub VecDeque<CiState>);

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

pub struct FakeWriter {
    pub receipts: Vec<DeployReceipt>,
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

pub fn fake_deploy_request(archive: PathBuf) -> DeployRequest {
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
        ci_repo: None,
        ci_branch: "main".into(),
        ci_max_attempts: 3,
        ci_poll_seconds: 1,
        stash_on_failure: true,
        dry_run: false,
    }
}

pub async fn make_archive() -> (TempDir, PathBuf, String) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("x.tar.gz");
    tokio::fs::write(&path, b"hello world").await.unwrap();
    let sha = jailgun_core::sha256_file(&path).unwrap();
    (dir, path, sha)
}

pub async fn archive_with_matching_upload() -> (TempDir, PathBuf, FakeUpload) {
    let (dir, archive, sha) = make_archive().await;
    (dir, archive, FakeUpload::new(vec![sha]))
}

pub fn ci_writer_events() -> (
    FakeCi,
    FakeWriter,
    broadcast::Sender<JailgunEvent>,
    broadcast::Receiver<JailgunEvent>,
) {
    let (tx, rx) = broadcast::channel(64);
    (
        FakeCi(vec![].into()),
        FakeWriter { receipts: vec![] },
        tx,
        rx,
    )
}

#[cfg(feature = "fake-backends")]
pub mod fake_e2e {
    use super::*;
    use jailgun_deploy::fake::{
        FakeCiTracker, FakeOutcome, FakeReceiptWriter, FakeRemoteJob, FakeRemoteUpload,
    };

    pub async fn make_source_archive(dir: &TempDir) -> (PathBuf, String) {
        let path = dir.path().join("source.tar.gz");
        tokio::fs::write(&path, b"fake source archive payload")
            .await
            .unwrap();
        let sha = jailgun_core::sha256_file(&path).unwrap();
        (path, sha)
    }

    pub fn request(archive: PathBuf, receipt_dir: PathBuf, dry_run: bool) -> DeployRequest {
        DeployRequest {
            run_id: "run-e2e".into(),
            tab_id: 1,
            remote_host: "fake-host".into(),
            remote_dir: "/srv/fake".into(),
            remote_command: "bash ci-fast-push.sh".into(),
            remote_archive_basename: "source.tar.gz".into(),
            local_archive_path: archive,
            strip_components: 1,
            cleanup_policy: CleanupPolicy::PreserveReset,
            receipt_dir,
            status_poll_seconds: 0,
            status_max_minutes: 1,
            ci_tracker_enabled: true,
            ci_repo: Some("neverhuman/jekko".into()),
            ci_branch: "main".into(),
            ci_max_attempts: 1,
            ci_poll_seconds: 0,
            stash_on_failure: true,
            dry_run,
        }
    }

    pub async fn deploy_with_fake_outcome(
        outcome: FakeOutcome,
        dry_run: bool,
    ) -> (TempDir, DeployReceipt, Vec<JailgunEvent>) {
        let dir = TempDir::new().unwrap();
        let (archive, sha) = make_source_archive(&dir).await;
        std::env::set_var("JAILGUN_FAKE_LOCAL_SHA", &sha);

        let mut upload = FakeRemoteUpload::new(outcome);
        let mut job = FakeRemoteJob::new(outcome);
        let mut ci = FakeCiTracker::new(outcome);
        let mut writer = FakeReceiptWriter::new(dir.path().join("receipts"));
        let (tx, mut rx) = broadcast::channel(64);

        let receipt = jailgun_deploy::deploy_remote(
            &mut upload,
            &mut job,
            &mut ci,
            &mut writer,
            request(archive, dir.path().join("receipts"), dry_run),
            &tx,
        )
        .await
        .expect("fake deploy ok");

        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }

        std::env::remove_var("JAILGUN_FAKE_LOCAL_SHA");
        (dir, receipt, events)
    }
}

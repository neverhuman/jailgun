//! FIFO deploy queue with bounded concurrency.
//!
//! Each `DeployJob` runs through `jailgun_deploy::cleanup_remote_checkout`
//! followed by `jailgun_deploy::deploy_remote`. Backends are pluggable so
//! tests can drive the queue with fakes and CI can flip to
//! `jailgun-deploy/fake-backends` for the e2e lane without touching code.

use std::{path::PathBuf, sync::Arc};

use jailgun_core::{CleanupPolicy, JailgunEvent};
use jailgun_deploy::{
    cleanup::{CleanupRequest, RemoteGitBackend},
    cleanup_remote_checkout,
    deploy::{deploy_remote, DeployError, DeployReceipt, DeployRequest, JsonReceiptWriter},
    CiTracker, RemoteJobBackend, RemoteUploadBackend,
};
use tokio::sync::{broadcast, mpsc, Semaphore};

#[derive(Debug, Clone)]
pub struct DeployJob {
    pub run_id: String,
    pub tab_id: u16,
    pub archive_path: PathBuf,
    pub remote_host: String,
    pub remote_dir: String,
    pub remote_command: String,
    pub remote_archive_basename: String,
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

/// Holds the four backends + the broadcast sender + a bounded concurrency
/// semaphore. One queue per orchestrator run. Spawn `run_deploy_queue` to
/// drain the inbound channel until shutdown.
pub struct DeployQueue<G, U, J, C, W> {
    pub git: G,
    pub upload: U,
    pub job: J,
    pub ci: C,
    pub writer: W,
    pub events: broadcast::Sender<JailgunEvent>,
    pub concurrency: Arc<Semaphore>,
}

pub async fn run_deploy_queue<G, U, J, C, W>(
    mut queue: DeployQueue<G, U, J, C, W>,
    mut inbox: mpsc::Receiver<DeployJob>,
) where
    G: RemoteGitBackend + Send + 'static,
    U: RemoteUploadBackend + Send + 'static,
    J: RemoteJobBackend + Send + 'static,
    C: CiTracker + Send + 'static,
    W: JsonReceiptWriter + Send + 'static,
{
    while let Some(job) = inbox.recv().await {
        let permit = queue.concurrency.clone().acquire_owned().await;
        let permit = match permit {
            Ok(p) => p,
            Err(_) => {
                tracing::warn!("deploy queue semaphore closed; skipping job");
                continue;
            }
        };
        if let Err(error) = process_one_job(&mut queue, &job).await {
            tracing::error!(?error, tab = job.tab_id, "deploy job failed");
        }
        drop(permit);
    }
}

async fn process_one_job<G, U, J, C, W>(
    queue: &mut DeployQueue<G, U, J, C, W>,
    job: &DeployJob,
) -> Result<DeployReceipt, DeployError>
where
    G: RemoteGitBackend + Send,
    U: RemoteUploadBackend + Send,
    J: RemoteJobBackend + Send,
    C: CiTracker + Send,
    W: JsonReceiptWriter + Send,
{
    let cleanup_req = CleanupRequest {
        run_id: job.run_id.clone(),
        tab_id: Some(job.tab_id),
        remote_host: job.remote_host.clone(),
        remote_dir: job.remote_dir.clone(),
        policy: job.cleanup_policy,
        receipt_dir: job.receipt_dir.clone(),
    };
    let _cleanup_receipt = cleanup_remote_checkout(&mut queue.git, cleanup_req).await?;

    let deploy_req = DeployRequest {
        run_id: job.run_id.clone(),
        tab_id: job.tab_id,
        remote_host: job.remote_host.clone(),
        remote_dir: job.remote_dir.clone(),
        remote_command: job.remote_command.clone(),
        remote_archive_basename: job.remote_archive_basename.clone(),
        local_archive_path: job.archive_path.clone(),
        strip_components: job.strip_components,
        cleanup_policy: job.cleanup_policy,
        receipt_dir: job.receipt_dir.clone(),
        status_poll_seconds: job.status_poll_seconds,
        status_max_minutes: job.status_max_minutes,
        ci_tracker_enabled: job.ci_tracker_enabled,
        ci_branch: job.ci_branch.clone(),
        ci_max_attempts: job.ci_max_attempts,
        ci_poll_seconds: job.ci_poll_seconds,
        stash_on_failure: job.stash_on_failure,
        dry_run: job.dry_run,
    };
    deploy_remote(
        &mut queue.upload,
        &mut queue.job,
        &mut queue.ci,
        &mut queue.writer,
        deploy_req,
        &queue.events,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use jailgun_deploy::{
        ci::CiState,
        cleanup::{CleanupError, CleanupReceipt, RemoteSnapshot},
        job::{JobHandle, JobPhase, JobSpec, JobStatus},
    };
    use std::{
        path::Path,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };
    use tempfile::TempDir;

    struct FakeGit {
        observe: Arc<AtomicUsize>,
    }
    #[async_trait]
    impl RemoteGitBackend for FakeGit {
        async fn snapshot(&mut self, _remote_dir: &str) -> Result<RemoteSnapshot, CleanupError> {
            Ok(RemoteSnapshot::clean("abc", "abc"))
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
        async fn write_receipt(
            &mut self,
            receipt: &CleanupReceipt,
        ) -> Result<PathBuf, CleanupError> {
            self.observe.fetch_add(1, Ordering::SeqCst);
            Ok(receipt
                .receipt_path
                .clone()
                .unwrap_or_else(|| PathBuf::from("cleanup-fake.json")))
        }
        async fn reset_hard(
            &mut self,
            _remote_dir: &str,
            _target: &str,
        ) -> Result<(), CleanupError> {
            Ok(())
        }
    }

    struct FakeUpload {
        sha: String,
    }
    #[async_trait]
    impl RemoteUploadBackend for FakeUpload {
        async fn ensure_remote_dir(&mut self, _remote_dir: &str) -> Result<(), DeployError> {
            Ok(())
        }
        async fn upload_archive(
            &mut self,
            _local: &Path,
            _remote: &str,
        ) -> Result<(), DeployError> {
            Ok(())
        }
        async fn remote_sha256(&mut self, _remote: &str) -> Result<String, DeployError> {
            Ok(self.sha.clone())
        }
        async fn remove_remote_file(&mut self, _remote: &str) -> Result<(), DeployError> {
            Ok(())
        }
    }

    struct FakeJob {
        observe: Arc<AtomicUsize>,
    }
    #[async_trait]
    impl RemoteJobBackend for FakeJob {
        async fn install_launcher(&mut self, spec: &JobSpec) -> Result<JobHandle, DeployError> {
            self.observe.fetch_add(1, Ordering::SeqCst);
            Ok(JobHandle {
                job_id: format!("{}-tab-{:02}", spec.run_id, spec.tab_id),
                launcher_dir: "/tmp/job".into(),
                launcher_path: "/tmp/job/launch.sh".into(),
                status_path: "/tmp/job/status.json".into(),
                log_path: "/tmp/job/launch.log".into(),
                failure_marker_path: "/tmp/job/deploy.failed".into(),
            })
        }
        async fn start_job(
            &mut self,
            _spec: &JobSpec,
            _handle: &JobHandle,
        ) -> Result<(), DeployError> {
            Ok(())
        }
        async fn fetch_status(&mut self, _handle: &JobHandle) -> Result<JobStatus, DeployError> {
            Ok(JobStatus {
                phase: JobPhase::Done,
                exit_code: Some(0),
                pre_head: Some("abc".into()),
                post_head: Some("abc".into()),
                ..Default::default()
            })
        }
        async fn fetch_log_tail(
            &mut self,
            _handle: &JobHandle,
            _n: usize,
        ) -> Result<String, DeployError> {
            Ok("ok".into())
        }
    }

    struct FakeCi;
    #[async_trait]
    impl CiTracker for FakeCi {
        async fn check(&mut self, _sha: &str, _branch: &str) -> Result<CiState, DeployError> {
            Ok(CiState::Skipped {
                reason: "test".into(),
            })
        }
        async fn capture_failure_log(
            &mut self,
            _run_id: &str,
            _max: usize,
        ) -> Result<String, DeployError> {
            Ok(String::new())
        }
    }

    struct FakeWriter {
        receipts: Arc<AtomicUsize>,
    }
    #[async_trait]
    impl JsonReceiptWriter for FakeWriter {
        async fn write_receipt(&mut self, receipt: &DeployReceipt) -> Result<PathBuf, DeployError> {
            self.receipts.fetch_add(1, Ordering::SeqCst);
            Ok(PathBuf::from(format!(
                "{}-tab-{:02}.json",
                receipt.run_id, receipt.tab_id
            )))
        }
    }

    #[tokio::test]
    async fn queue_drains_in_order_with_concurrency_one() {
        let dir = TempDir::new().unwrap();
        let archive = dir.path().join("x.tar.gz");
        tokio::fs::write(&archive, b"data").await.unwrap();
        let sha = jailgun_core::sha256_file(&archive).unwrap();

        let receipts = Arc::new(AtomicUsize::new(0));
        let install_calls = Arc::new(AtomicUsize::new(0));
        let cleanup_calls = Arc::new(AtomicUsize::new(0));
        let (tx, _rx) = broadcast::channel(64);
        let queue = DeployQueue {
            git: FakeGit {
                observe: cleanup_calls.clone(),
            },
            upload: FakeUpload { sha: sha.clone() },
            job: FakeJob {
                observe: install_calls.clone(),
            },
            ci: FakeCi,
            writer: FakeWriter {
                receipts: receipts.clone(),
            },
            events: tx,
            concurrency: Arc::new(Semaphore::new(1)),
        };
        let (job_tx, job_rx) = mpsc::channel::<DeployJob>(8);
        let driver = tokio::spawn(run_deploy_queue(queue, job_rx));

        for tab in 0..3 {
            job_tx
                .send(DeployJob {
                    run_id: "run-q".into(),
                    tab_id: tab,
                    archive_path: archive.clone(),
                    remote_host: "h".into(),
                    remote_dir: "/srv/x".into(),
                    remote_command: "true".into(),
                    remote_archive_basename: "x.tar.gz".into(),
                    strip_components: 1,
                    cleanup_policy: CleanupPolicy::PreserveReset,
                    receipt_dir: dir.path().to_path_buf(),
                    status_poll_seconds: 0,
                    status_max_minutes: 1,
                    ci_tracker_enabled: false,
                    ci_branch: "main".into(),
                    ci_max_attempts: 1,
                    ci_poll_seconds: 1,
                    stash_on_failure: true,
                    dry_run: false,
                })
                .await
                .unwrap();
        }
        drop(job_tx);
        driver.await.unwrap();
        assert_eq!(receipts.load(Ordering::SeqCst), 3);
        assert_eq!(install_calls.load(Ordering::SeqCst), 3);
        assert_eq!(cleanup_calls.load(Ordering::SeqCst), 3);
    }
}

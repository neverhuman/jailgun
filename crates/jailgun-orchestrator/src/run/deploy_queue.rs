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

#[cfg(test)]
mod tests;

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
    pub ci_repo: Option<String>,
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
        ci_repo: job.ci_repo.clone(),
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

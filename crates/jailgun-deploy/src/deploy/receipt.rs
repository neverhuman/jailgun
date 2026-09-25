use crate::{
    ci::CiState,
    job::{JobHandle, JobStatus},
};

use super::{DeployOutcome, DeployReceipt, DeployRequest};

#[allow(clippy::too_many_arguments)]
pub(super) fn build_receipt(
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
        ci_repo: req.ci_repo.clone(),
        log_tail,
        outcome,
        receipt_path: None,
    }
}

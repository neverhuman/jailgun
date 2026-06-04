use async_trait::async_trait;

use crate::{
    deploy::DeployError,
    job::{JobHandle, JobPhase, JobSpec, JobStatus, RemoteJobBackend},
};

use super::FakeOutcome;

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

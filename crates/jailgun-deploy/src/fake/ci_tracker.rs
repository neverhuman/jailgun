use async_trait::async_trait;

use crate::{
    ci::{CiState, CiTracker},
    deploy::DeployError,
};

use super::FakeOutcome;

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

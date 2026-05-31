//! GitHub Actions CI tracking trait + state.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::deploy::DeployError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CiState {
    Unknown,
    Pending {
        #[serde(default)]
        run_id: Option<String>,
    },
    Passed {
        run_id: String,
        run_url: String,
        conclusion: String,
    },
    Failed {
        run_id: String,
        run_url: String,
        conclusion: String,
        #[serde(default)]
        log_excerpt: Option<String>,
    },
    Skipped {
        reason: String,
    },
}

impl CiState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            CiState::Passed { .. } | CiState::Failed { .. } | CiState::Skipped { .. }
        )
    }
}

#[async_trait]
pub trait CiTracker {
    async fn check(&mut self, commit_sha: &str, branch: &str) -> Result<CiState, DeployError>;
    async fn capture_failure_log(
        &mut self,
        run_id: &str,
        max_bytes: usize,
    ) -> Result<String, DeployError>;
}

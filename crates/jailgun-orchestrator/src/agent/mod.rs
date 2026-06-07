mod accounts;
mod execute;
mod execute_summary;
mod prepare;
mod prepare_env;
mod review;

use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use jailgun_core::{
    BrowserLeaseRequest, JailgunAgentRunRequest, JailgunAgentRunSummary, JailgunConfig,
    JailgunEvent,
};

use crate::{
    config::RunOptions,
    run::{run_orchestration, OrchestratorHandle},
};

pub use execute::execute_prepared_agent_run;
pub use prepare::{prepare_agent_run, run_agent};
pub use review::build_review_packet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunPaths {
    pub events_jsonl: PathBuf,
    pub summary_json: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PreparedAgentRun {
    pub request: JailgunAgentRunRequest,
    pub config: JailgunConfig,
    pub repo_url: String,
    pub tabs: u16,
    pub max_runtime_seconds: u64,
    pub deploy_expected_top_level: Option<String>,
    pub started_at: String,
    pub prompt_text: String,
    pub output_paths: AgentRunPaths,
    pub opts: RunOptions,
    pub browser_lease: Option<PreparedBrowserLease>,
}

#[derive(Debug, Clone)]
pub struct PreparedBrowserLease {
    pub registry_path: PathBuf,
    pub request: BrowserLeaseRequest,
}

#[async_trait]
pub trait AgentRunBackend: Send + Sync {
    async fn start(&self, opts: RunOptions) -> Result<OrchestratorHandle>;
}

pub struct DefaultAgentRunBackend;

#[async_trait]
impl AgentRunBackend for DefaultAgentRunBackend {
    async fn start(&self, opts: RunOptions) -> Result<OrchestratorHandle> {
        Ok(run_orchestration(opts).await?)
    }
}

#[async_trait]
pub trait AgentRunEventSink: Send + Sync {
    async fn on_event(&self, _event: &JailgunEvent) -> Result<()> {
        Ok(())
    }

    async fn on_summary(&self, _summary: &JailgunAgentRunSummary) -> Result<()> {
        Ok(())
    }
}

pub struct NoopAgentRunEventSink;

#[async_trait]
impl AgentRunEventSink for NoopAgentRunEventSink {}

pub(super) fn timestamp_now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod account_tests;
#[cfg(test)]
mod tests;

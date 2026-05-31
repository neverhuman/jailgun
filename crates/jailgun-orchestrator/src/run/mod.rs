//! Run lifecycle: supervisor + per-tab actors + deploy queue.
//!
//! Phase 1 ships the public surface and the `TabState` machine. The
//! supervisor + queue implementations land in the next pass once
//! `jailgun-deploy::deploy_remote` is ready (Task #5).

pub mod tab;

pub use tab::{TabState, TabTransitionError};

use serde::{Deserialize, Serialize};

use crate::{config::RunOptions, errors::OrchestratorError};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunSummary {
    pub run_id: String,
    pub total_tabs: u16,
    pub downloaded: u16,
    pub deployed: u16,
    pub failures: Vec<(u16, String)>,
    pub denied_github_prompts: u32,
    pub allowed_info_prompts: u32,
}

pub struct OrchestratorHandle {
    pub events_rx: tokio::sync::broadcast::Receiver<jailgun_core::JailgunEvent>,
    pub completion: tokio::sync::oneshot::Receiver<RunSummary>,
    pub shutdown: tokio::sync::watch::Sender<bool>,
}

/// Top-level entry; the body is intentionally a stub until the bridge IO and
/// deploy execution layers land. Calling it today returns a placeholder error
/// so callers can wire the API surface without crashing on `unimplemented!()`.
pub async fn run_orchestration(_opts: RunOptions) -> Result<OrchestratorHandle, OrchestratorError> {
    Err(OrchestratorError::Config(
        "run_orchestration not yet implemented; bridge + deploy execution wiring still in progress"
            .into(),
    ))
}

pub mod deploy_queue;
pub mod events;
pub use deploy_queue::{run_deploy_queue, DeployJob, DeployQueue};
pub use events::map_bridge_event;

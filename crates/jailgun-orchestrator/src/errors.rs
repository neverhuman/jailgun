use jailgun_core::{AgentError, AgentErrorExt};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("bridge spawn failed: {0}")]
    BridgeSpawn(String),
    #[error("bridge exited unexpectedly with status {0:?}")]
    BridgeExited(Option<i32>),
    #[error("bridge handshake timed out after {0} seconds")]
    HandshakeTimeout(u64),
    #[error("bridge protocol error: {0}")]
    Protocol(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("tab {tab_id} failed: {reason}")]
    Tab { tab_id: u16, reason: String },
    #[error("run cancelled")]
    Cancelled,
    #[error("config error: {0}")]
    Config(String),
}

impl AgentErrorExt for OrchestratorError {
    fn agent_error(&self) -> AgentError {
        let code = match self {
            OrchestratorError::BridgeSpawn(_) => "orchestrator-bridge-spawn",
            OrchestratorError::BridgeExited(_) => "orchestrator-bridge-exited",
            OrchestratorError::HandshakeTimeout(_) => "orchestrator-handshake-timeout",
            OrchestratorError::Protocol(_) => "orchestrator-protocol",
            OrchestratorError::Io(_) => "orchestrator-io",
            OrchestratorError::Serde(_) => "orchestrator-serde",
            OrchestratorError::Tab { .. } => "orchestrator-tab",
            OrchestratorError::Cancelled => "orchestrator-cancelled",
            OrchestratorError::Config(_) => "orchestrator-config",
        };
        AgentError::new(
            code,
            "coordinate browser bridge and run lifecycle",
            self.to_string(),
            vec![
                "check bridge command and environment config",
                "inspect NDJSON protocol frames",
                "rerun orchestrator bridge tests with bounded queues",
            ],
            "docs/testing.md",
            "rerun `cargo test -p jailgun-orchestrator`",
        )
    }
}

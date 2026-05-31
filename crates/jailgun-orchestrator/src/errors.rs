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

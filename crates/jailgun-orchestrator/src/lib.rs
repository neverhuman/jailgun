pub mod agent;
pub mod bridge;
pub mod config;
pub mod errors;
pub mod run;
mod support;

pub use agent::{
    build_review_packet, execute_prepared_agent_run, prepare_agent_run, run_agent,
    AgentRunBackend, AgentRunEventSink, AgentRunPaths, DefaultAgentRunBackend, PreparedAgentRun,
};
pub use config::RunOptions;
pub use errors::OrchestratorError;
pub use run::{run_orchestration, OrchestratorHandle, RunSummary, TabState};

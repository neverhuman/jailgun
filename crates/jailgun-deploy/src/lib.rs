pub mod ci;
pub mod cleanup;
pub mod deploy;
#[cfg(feature = "fake-backends")]
pub mod fake;
pub mod job;
pub mod launcher;
pub mod shell;
pub mod upload;
mod util;

pub use ci::{CiState, CiTracker};
pub use cleanup::{
    cleanup_remote_checkout, CleanupError, CleanupOutcome, CleanupReceipt, CleanupRequest,
    RemoteGitBackend, RemoteSnapshot,
};
pub use deploy::{
    deploy_remote, DeployError, DeployOutcome, DeployReceipt, DeployRequest, JsonReceiptWriter,
};
pub use job::{JobHandle, JobPhase, JobSpec, JobStatus, RemoteJobBackend};
pub use launcher::{build_launcher_script, parse_status_json, LAUNCHER_SCHEMA_VERSION};
pub use upload::RemoteUploadBackend;

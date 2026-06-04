use std::{path::Path, process::Stdio};

use tokio::process::Command;

use crate::deploy::DeployError;

mod ci_tracker;
mod git;
mod job;
mod upload;

pub use ci_tracker::SshCiTracker;
pub use git::SshRemoteGit;
pub use job::SshRemoteJob;
pub use upload::SshRemoteUpload;

pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

struct RemoteCommandError(String);

impl std::fmt::Display for RemoteCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

async fn run_ssh_command(host: &str, script: &str) -> Result<String, RemoteCommandError> {
    let output = Command::new("ssh")
        .arg(host)
        .arg(script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|error| RemoteCommandError(format!("ssh failed to start: {error}")))?;
    if !output.status.success() {
        return Err(RemoteCommandError(format!(
            "ssh exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn run_deploy_ssh(host: &str, script: &str) -> Result<String, DeployError> {
    run_ssh_command(host, script)
        .await
        .map_err(|error| DeployError::Ssh(error.to_string()))
}

#[allow(dead_code)]
fn ensure_absolute(path: &Path) -> bool {
    path.is_absolute()
}

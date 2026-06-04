use std::{path::Path, process::Stdio};

use async_trait::async_trait;
use tokio::process::Command;

use crate::{deploy::DeployError, upload::RemoteUploadBackend};

use super::{run_deploy_ssh, shell_quote};

pub struct SshRemoteUpload {
    host: String,
}

impl SshRemoteUpload {
    pub fn new(host: impl Into<String>) -> Self {
        Self { host: host.into() }
    }
}

#[async_trait]
impl RemoteUploadBackend for SshRemoteUpload {
    async fn ensure_remote_dir(&mut self, remote_dir: &str) -> Result<(), DeployError> {
        run_deploy_ssh(&self.host, &format!("mkdir -p {}", shell_quote(remote_dir))).await?;
        Ok(())
    }

    async fn upload_archive(
        &mut self,
        local_path: &Path,
        remote_path: &str,
    ) -> Result<(), DeployError> {
        let output = Command::new("scp")
            .arg(local_path)
            .arg(format!("{}:{}", self.host, remote_path))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|error| DeployError::Scp(format!("scp failed to start: {error}")))?;
        if !output.status.success() {
            return Err(DeployError::Scp(format!(
                "scp exited {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(())
    }

    async fn remote_sha256(&mut self, remote_path: &str) -> Result<String, DeployError> {
        let output = run_deploy_ssh(
            &self.host,
            &format!(
                "sha256sum {} | awk '{{print $1}}'",
                shell_quote(remote_path)
            ),
        )
        .await?;
        Ok(output.lines().next().unwrap_or_default().trim().to_string())
    }

    async fn remove_remote_file(&mut self, remote_path: &str) -> Result<(), DeployError> {
        run_deploy_ssh(&self.host, &format!("rm -f {}", shell_quote(remote_path))).await?;
        Ok(())
    }
}

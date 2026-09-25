use std::path::PathBuf;

use async_trait::async_trait;
use tokio::{fs, io::AsyncWriteExt};

use crate::cleanup::{CleanupError, CleanupReceipt, RemoteGitBackend, RemoteSnapshot};

use super::{run_ssh_command, shell_quote};

pub struct SshRemoteGit {
    host: String,
    receipt_dir: PathBuf,
}

impl SshRemoteGit {
    pub fn new(host: impl Into<String>, receipt_dir: impl Into<PathBuf>) -> Self {
        Self {
            host: host.into(),
            receipt_dir: receipt_dir.into(),
        }
    }

    async fn run_script(&self, remote_dir: &str, script: &str) -> Result<String, CleanupError> {
        let remote_command = format!("cd {} && {}", shell_quote(remote_dir), script);
        run_ssh_command(&self.host, &remote_command)
            .await
            .map_err(|error| CleanupError::Backend(error.to_string()))
    }
}

#[async_trait]
impl RemoteGitBackend for SshRemoteGit {
    async fn snapshot(&mut self, remote_dir: &str) -> Result<RemoteSnapshot, CleanupError> {
        let output = self
            .run_script(
                remote_dir,
                "printf 'head=%s\\n' \"$(git rev-parse HEAD 2>/dev/null || true)\"; \
                 printf 'origin_main=%s\\n' \"$(git rev-parse origin/main 2>/dev/null || true)\"; \
                 printf '__STATUS__\\n'; git status --short 2>/dev/null || true",
            )
            .await?;
        let (meta, status) = output
            .split_once("__STATUS__\n")
            .unwrap_or((output.as_str(), ""));
        let mut head = None;
        let mut origin_main = None;
        for line in meta.lines() {
            if let Some(value) = line.strip_prefix("head=") {
                if !value.trim().is_empty() {
                    head = Some(value.trim().into());
                }
            }
            if let Some(value) = line.strip_prefix("origin_main=") {
                if !value.trim().is_empty() {
                    origin_main = Some(value.trim().into());
                }
            }
        }
        Ok(RemoteSnapshot {
            head,
            origin_main,
            status_short: status.trim().into(),
        })
    }

    async fn fetch_origin(&mut self, remote_dir: &str) -> Result<(), CleanupError> {
        self.run_script(remote_dir, "git fetch origin").await?;
        Ok(())
    }

    async fn create_ref(
        &mut self,
        remote_dir: &str,
        ref_name: &str,
        sha: &str,
    ) -> Result<(), CleanupError> {
        self.run_script(
            remote_dir,
            &format!(
                "git update-ref {} {}",
                shell_quote(ref_name),
                shell_quote(sha)
            ),
        )
        .await?;
        Ok(())
    }

    async fn write_receipt(&mut self, receipt: &CleanupReceipt) -> Result<PathBuf, CleanupError> {
        let path = match receipt.receipt_path.clone() {
            Some(path) => path,
            None => self
                .receipt_dir
                .join(format!("{}-remote-cleanup.json", receipt.run_id)),
        };
        let Some(parent) = path.parent() else {
            return Err(CleanupError::Receipt("receipt path has no parent".into()));
        };
        fs::create_dir_all(parent)
            .await
            .map_err(|error| CleanupError::Receipt(error.to_string()))?;
        let bytes = serde_json::to_vec_pretty(receipt)
            .map_err(|error| CleanupError::Receipt(error.to_string()))?;
        let mut file = fs::File::create(&path)
            .await
            .map_err(|error| CleanupError::Receipt(error.to_string()))?;
        file.write_all(&bytes)
            .await
            .map_err(|error| CleanupError::Receipt(error.to_string()))?;
        Ok(path)
    }

    async fn reset_hard(&mut self, remote_dir: &str, target: &str) -> Result<(), CleanupError> {
        self.run_script(
            remote_dir,
            &format!("git reset --hard {} && git clean -fd", shell_quote(target)),
        )
        .await?;
        Ok(())
    }
}

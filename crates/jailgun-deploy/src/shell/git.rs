use std::path::PathBuf;

use async_trait::async_trait;
use tokio::{fs, io::AsyncWriteExt};

use crate::cleanup::{CleanupError, CleanupReceipt, RemoteGitBackend, RemoteSnapshot};

use super::{run_ssh_command, shell_quote};

#[cfg(test)]
mod tests;

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
                 printf '__STATUS__\\n'; git status --short --untracked-files=all",
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
                "git update-ref {} {} ''",
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
        let mut directory = std::fs::DirBuilder::new();
        directory.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            directory.mode(0o700);
        }
        directory
            .create(parent)
            .map_err(|error| CleanupError::Receipt(error.to_string()))?;
        let bytes = serde_json::to_vec_pretty(receipt)
            .map_err(|error| CleanupError::Receipt(error.to_string()))?;
        let staging = path.with_extension("json.writing");
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            options.mode(0o600);
        }
        let mut file = options
            .open(&staging)
            .await
            .map_err(|error| CleanupError::Receipt(error.to_string()))?;
        file.write_all(&bytes)
            .await
            .map_err(|error| CleanupError::Receipt(error.to_string()))?;
        file.sync_all()
            .await
            .map_err(|error| CleanupError::Receipt(error.to_string()))?;
        fs::rename(&staging, &path)
            .await
            .map_err(|error| CleanupError::Receipt(error.to_string()))?;
        std::fs::File::open(parent)
            .and_then(|parent| parent.sync_all())
            .map_err(|error| CleanupError::Receipt(error.to_string()))?;
        Ok(path)
    }

    async fn reset_preserved(
        &mut self,
        remote_dir: &str,
        expected_head: &str,
        preserved_ref: &str,
        target: &str,
    ) -> Result<(), CleanupError> {
        self.run_script(
            remote_dir,
            &guarded_reset_script(expected_head, preserved_ref, target),
        )
        .await?;
        Ok(())
    }
}

pub(crate) fn guarded_reset_script(
    expected_head: &str,
    preserved_ref: &str,
    target: &str,
) -> String {
    format!(
        r#"set -eu
umask 077
git_dir=$(git rev-parse --absolute-git-dir)
mutation_lock="$git_dir/jailgun-mutation.lock"
mkdir "$mutation_lock" || {{ echo 'checkout-owned: another mutation owns this checkout' >&2; exit 47; }}
trap 'rmdir "$mutation_lock"' EXIT
test "$(git rev-parse --verify HEAD)" = {head}
test "$(git rev-parse --verify {preserved})" = {head}
test "$(git rev-parse --verify origin/main)" = {target}
checkout_status=$(git status --porcelain=v1 --untracked-files=all)
test -z "$checkout_status"
git reset --hard {target}
"#,
        head = shell_quote(expected_head),
        preserved = shell_quote(preserved_ref),
        target = shell_quote(target)
    )
}

use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use async_trait::async_trait;
use serde::Deserialize;
use tokio::{fs, io::AsyncWriteExt, process::Command};

use crate::{
    ci::{CiState, CiTracker},
    cleanup::{CleanupError, CleanupReceipt, RemoteGitBackend, RemoteSnapshot},
    deploy::DeployError,
    job::{JobHandle, JobSpec, JobStatus, RemoteJobBackend},
    launcher::{build_launcher_script, parse_status_json},
    upload::RemoteUploadBackend,
    util::sanitize_ref_fragment,
};

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

pub struct SshRemoteUpload {
    host: String,
}

macro_rules! ssh_host_constructor {
    ($type_name:ident) => {
        impl $type_name {
            pub fn new(host: impl Into<String>) -> Self {
                Self { host: host.into() }
            }
        }
    };
}

ssh_host_constructor!(SshRemoteUpload);

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

pub struct SshRemoteJob {
    host: String,
}

ssh_host_constructor!(SshRemoteJob);

pub struct SshCiTracker;

impl SshCiTracker {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SshCiTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RemoteJobBackend for SshRemoteJob {
    async fn install_launcher(&mut self, spec: &JobSpec) -> Result<JobHandle, DeployError> {
        let job_id = format!(
            "{}-tab-{:02}",
            sanitize_ref_fragment(&spec.run_id),
            spec.tab_id
        );
        let launcher_dir = format!("/tmp/jailgun-runs/{job_id}");
        let launcher_path = format!("{launcher_dir}/launcher.sh");
        let status_path = format!("{launcher_dir}/status.json");
        let log_path = format!("{launcher_dir}/launch.log");
        let failure_marker_path = format!("{launcher_dir}/deploy.failed");
        let script = build_launcher_script(spec);

        let mut child = Command::new("ssh")
            .arg(&self.host)
            .arg(format!(
                "mkdir -p {} && cat > {} && chmod +x {}",
                shell_quote(&launcher_dir),
                shell_quote(&launcher_path),
                shell_quote(&launcher_path)
            ))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                DeployError::LauncherInstall(format!("ssh failed to start: {error}"))
            })?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| DeployError::LauncherInstall("missing ssh stdin".into()))?;
        stdin
            .write_all(script.as_bytes())
            .await
            .map_err(|error| DeployError::LauncherInstall(error.to_string()))?;
        drop(stdin);
        let output = child
            .wait_with_output()
            .await
            .map_err(|error| DeployError::LauncherInstall(error.to_string()))?;
        if !output.status.success() {
            return Err(DeployError::LauncherInstall(format!(
                "ssh exited {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }

        Ok(JobHandle {
            job_id,
            launcher_dir,
            launcher_path,
            status_path,
            log_path,
            failure_marker_path,
        })
    }

    async fn start_job(&mut self, _spec: &JobSpec, handle: &JobHandle) -> Result<(), DeployError> {
        run_deploy_ssh(
            &self.host,
            &format!(
                "nohup {} > {} 2>&1 < /dev/null &",
                shell_quote(&handle.launcher_path),
                shell_quote(&handle.log_path)
            ),
        )
        .await
        .map_err(|error| DeployError::LauncherStart(error.to_string()))?;
        Ok(())
    }

    async fn fetch_status(&mut self, handle: &JobHandle) -> Result<JobStatus, DeployError> {
        let output = run_deploy_ssh(
            &self.host,
            &format!(
                "cat {} 2>/dev/null || true",
                shell_quote(&handle.status_path)
            ),
        )
        .await
        .map_err(|error| DeployError::StatusFetch(error.to_string()))?;
        if output.trim().is_empty() {
            return Ok(JobStatus::default());
        }
        parse_status_json(&output).map_err(|error| DeployError::StatusParse(error.to_string()))
    }

    async fn fetch_log_tail(
        &mut self,
        handle: &JobHandle,
        last_n_lines: usize,
    ) -> Result<String, DeployError> {
        run_deploy_ssh(
            &self.host,
            &format!(
                "tail -n {} {} 2>/dev/null || true",
                last_n_lines,
                shell_quote(&handle.log_path)
            ),
        )
        .await
        .map_err(|error| DeployError::LogFetch(error.to_string()))
    }
}

#[async_trait]
impl CiTracker for SshCiTracker {
    async fn check(&mut self, commit_sha: &str, branch: &str) -> Result<CiState, DeployError> {
        let output = Command::new("gh")
            .args([
                "run",
                "list",
                "--branch",
                branch,
                "--commit",
                commit_sha,
                "--json",
                "databaseId,status,conclusion,url",
                "--limit",
                "1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;
        let output = match output {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(CiState::Skipped {
                    reason: "gh-not-on-path".into(),
                });
            }
            Err(error) => {
                return Err(DeployError::CiTracker(format!(
                    "gh failed to start: {error}"
                )))
            }
        };
        if !output.status.success() {
            return Err(DeployError::CiTracker(format!(
                "gh run list exited {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        parse_gh_run_list(&output.stdout)
    }

    async fn capture_failure_log(
        &mut self,
        run_id: &str,
        max_bytes: usize,
    ) -> Result<String, DeployError> {
        let output = Command::new("gh")
            .args(["run", "view", run_id, "--log-failed"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;
        let output = match output {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
            Err(error) => {
                return Err(DeployError::CiTracker(format!(
                    "gh failed to start: {error}"
                )))
            }
        };
        if !output.status.success() {
            return Err(DeployError::CiTracker(format!(
                "gh run view exited {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        let mut text = String::from_utf8_lossy(&output.stdout).to_string();
        if text.len() > max_bytes {
            let start = text.len().saturating_sub(max_bytes);
            text = text[start..].to_string();
        }
        Ok(text)
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

fn shell_quote(value: &str) -> String {
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhRun {
    database_id: Option<u64>,
    status: Option<String>,
    conclusion: Option<String>,
    url: Option<String>,
}

fn parse_gh_run_list(bytes: &[u8]) -> Result<CiState, DeployError> {
    let runs: Vec<GhRun> = serde_json::from_slice(bytes)?;
    let Some(run) = runs.into_iter().next() else {
        return Ok(CiState::Pending { run_id: None });
    };
    let run_id = run.database_id.map(|id| id.to_string()).unwrap_or_default();
    let url = run.url.unwrap_or_default();
    let status = run.status.unwrap_or_default();
    let conclusion = run.conclusion.unwrap_or_default();
    if status != "completed" {
        return Ok(CiState::Pending {
            run_id: if run_id.is_empty() {
                None
            } else {
                Some(run_id)
            },
        });
    }
    if conclusion == "success" {
        Ok(CiState::Passed {
            run_id,
            run_url: url,
            conclusion,
        })
    } else {
        Ok(CiState::Failed {
            run_id,
            run_url: url,
            conclusion,
            log_excerpt: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_gh_run_list_returns_pending_when_no_run_exists_yet() {
        let state = parse_gh_run_list(br#"[]"#).unwrap();
        assert_eq!(state, CiState::Pending { run_id: None });
    }

    #[test]
    fn parse_gh_run_list_maps_successful_completed_run() {
        let state = parse_gh_run_list(
            br#"[{"databaseId":42,"status":"completed","conclusion":"success","url":"https://example.test/run"}]"#,
        )
        .unwrap();
        assert_eq!(
            state,
            CiState::Passed {
                run_id: "42".into(),
                run_url: "https://example.test/run".into(),
                conclusion: "success".into()
            }
        );
    }

    #[test]
    fn parse_gh_run_list_maps_failed_completed_run() {
        let state = parse_gh_run_list(
            br#"[{"databaseId":99,"status":"completed","conclusion":"failure","url":"https://example.test/run"}]"#,
        )
        .unwrap();
        assert_eq!(
            state,
            CiState::Failed {
                run_id: "99".into(),
                run_url: "https://example.test/run".into(),
                conclusion: "failure".into(),
                log_excerpt: None
            }
        );
    }
}

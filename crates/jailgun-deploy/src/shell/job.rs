use std::process::Stdio;

use async_trait::async_trait;
use tokio::{io::AsyncWriteExt, process::Command};

use crate::{
    deploy::DeployError,
    job::{JobHandle, JobSpec, JobStatus, RemoteJobBackend},
    launcher::{build_launcher_script, parse_status_json},
    util::sanitize_ref_fragment,
};

use super::{run_deploy_ssh, shell_quote};

pub struct SshRemoteJob {
    host: String,
}

impl SshRemoteJob {
    pub fn new(host: impl Into<String>) -> Self {
        Self { host: host.into() }
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

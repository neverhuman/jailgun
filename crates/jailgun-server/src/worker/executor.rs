use super::*;

#[derive(Debug, Clone)]
pub struct ProcessExecutor {
    program: PathBuf,
}

impl ProcessExecutor {
    pub fn new(program: PathBuf) -> Self {
        Self { program }
    }
}

#[async_trait]
impl WorkerExecutor for ProcessExecutor {
    fn name(&self) -> &'static str {
        "codex-process"
    }

    async fn auth_status(&self, codex_home: PathBuf) -> WorkerAccountStatus {
        let status = tokio::time::timeout(
            Duration::from_secs(AUTH_CHECK_TIMEOUT_SECONDS),
            Command::new(&self.program)
                .arg("login")
                .arg("status")
                .env("CODEX_HOME", codex_home)
                .env_remove("OPENAI_API_KEY")
                .env_remove("CODEX_ACCESS_TOKEN")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status(),
        )
        .await;
        match status {
            Ok(Ok(status)) if status.success() => WorkerAccountStatus::Ready,
            Ok(Ok(_)) => WorkerAccountStatus::LoginRequired,
            _ => WorkerAccountStatus::Unavailable,
        }
    }

    async fn execute(
        &self,
        request: ExecutionRequest,
        mut cancel: watch::Receiver<bool>,
    ) -> std::result::Result<ExecutionResult, ExecutionFailure> {
        tokio::fs::create_dir_all(&request.log_dir)
            .await
            .map_err(io_failure)?;
        let stdout_path = request.log_dir.join("stdout.jsonl");
        let stderr_path = request.log_dir.join("stderr.log");
        let stdout = File::create(&stdout_path).map_err(io_failure)?;
        let stderr = File::create(&stderr_path).map_err(io_failure)?;
        let result_path = request.workspace.join("jailgun-result.md");
        let effort = format!(
            "model_reasoning_effort=\"{}\"",
            request.reasoning_effort.as_str()
        );
        let mut child = Command::new(&self.program)
            .arg("exec")
            .arg("--json")
            .arg("--ephemeral")
            .arg("--skip-git-repo-check")
            .arg("--sandbox")
            .arg("workspace-write")
            .arg("--model")
            .arg(&request.model)
            .arg("--config")
            .arg(effort)
            .arg("--cd")
            .arg(&request.workspace)
            .arg("--output-last-message")
            .arg(&result_path)
            .arg("-")
            .env("CODEX_HOME", &request.codex_home)
            .env_remove("OPENAI_API_KEY")
            .env_remove("CODEX_ACCESS_TOKEN")
            .stdin(Stdio::piped())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| ExecutionFailure {
                kind: ExecutionFailureKind::Failed,
                message: format!("executor-start-failed: {error}"),
            })?;
        let mut stdin = child.stdin.take().ok_or_else(|| ExecutionFailure {
            kind: ExecutionFailureKind::Failed,
            message: "executor-stdin-unavailable".into(),
        })?;
        stdin
            .write_all(request.prompt.as_bytes())
            .await
            .map_err(io_failure)?;
        drop(stdin);

        let deadline = tokio::time::sleep(Duration::from_secs(request.timeout_seconds));
        tokio::pin!(deadline);
        let status = tokio::select! {
            status = child.wait() => status.map_err(io_failure)?,
            _ = cancel.changed() => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                return Err(ExecutionFailure { kind: ExecutionFailureKind::Cancelled, message: "job cancelled".into() });
            }
            _ = &mut deadline => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                return Err(ExecutionFailure { kind: ExecutionFailureKind::TimedOut, message: format!("job exceeded {} seconds", request.timeout_seconds) });
            }
        };
        if status.success() {
            return Ok(ExecutionResult {
                exit_code: status.code().unwrap_or(0),
            });
        }
        let authentication_failed = stderr_indicates_auth_failure(&stderr_path);
        let detail = error_detail(&stderr_path, request.error_reporting);
        Err(ExecutionFailure {
            kind: if authentication_failed {
                ExecutionFailureKind::Authentication
            } else {
                ExecutionFailureKind::Failed
            },
            message: if detail.is_empty() {
                format!("executor exited with {}", status.code().unwrap_or(-1))
            } else {
                detail
            },
        })
    }
}

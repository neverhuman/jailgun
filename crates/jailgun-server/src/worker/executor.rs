use super::*;
use jailgun_orchestrator::concept::AccountSupervisor;
use jailgun_workflow::BrowserCapture;
use serde_json::json;

#[derive(Clone)]
pub struct BrowserExecutor {
    supervisor: AccountSupervisor,
}

impl BrowserExecutor {
    pub fn new(supervisor: AccountSupervisor) -> Self {
        Self { supervisor }
    }
}

#[async_trait]
impl WorkerExecutor for BrowserExecutor {
    fn name(&self) -> &'static str {
        "chatgpt-browser"
    }

    async fn auth_status(&self, account_id: &str) -> WorkerAccountStatus {
        match self.supervisor.store.accounts().await {
            Ok(accounts)
                if accounts
                    .iter()
                    .any(|account| account.id == account_id && account.readiness == "ready") =>
            {
                WorkerAccountStatus::Ready
            }
            Ok(accounts) if accounts.iter().any(|account| account.id == account_id) => {
                WorkerAccountStatus::LoginRequired
            }
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
        let result_path = request.workspace.join("jailgun-result.md");
        let bridge = self
            .supervisor
            .bridge_for_ready_account(&request.account_id)
            .await
            .ok_or_else(|| ExecutionFailure {
                kind: ExecutionFailureKind::Authentication,
                message: "account-login-required: reconnect this ChatGPT account in the Jailgun dashboard"
                    .into(),
            })?;
        let run_id = format!("worker-{}", request.job_id);
        let attempt_id = request.job_id.clone();
        let model = if request.model.eq_ignore_ascii_case("current") {
            json!({"mode":"current"})
        } else {
            json!({"mode":"specific","name":request.model})
        };
        let prepare = bridge
            .call(
                "concept.prepare",
                &run_id,
                json!({
                    "attempt_id": attempt_id,
                    "prompt": request.prompt,
                    "model": model,
                    "reasoning_effort": request.reasoning_effort.as_str(),
                    "attachment_path": request.input_archive_path,
                    "timeout_ms": request.timeout_seconds.saturating_mul(1000),
                }),
                Duration::from_secs(90),
            )
            .await;
        if let Err(error) = prepare {
            let _ = bridge
                .call(
                    "concept.close",
                    &run_id,
                    json!({"attempt_id":attempt_id,"confirmed_not_submitted":true}),
                    Duration::from_secs(10),
                )
                .await;
            return Err(browser_failure(error));
        }
        if *cancel.borrow() {
            let _ = bridge
                .call(
                    "concept.close",
                    &run_id,
                    json!({"attempt_id":attempt_id,"confirmed_not_submitted":true}),
                    Duration::from_secs(10),
                )
                .await;
            return Err(ExecutionFailure {
                kind: ExecutionFailureKind::Cancelled,
                message: "job cancelled before submission".into(),
            });
        }
        bridge
            .call(
                "concept.submit",
                &run_id,
                json!({"attempt_id":attempt_id}),
                Duration::from_secs(60),
            )
            .await
            .map_err(browser_failure)?;

        let capture = bridge.call(
            "concept.capture",
            &run_id,
            json!({"attempt_id":attempt_id}),
            Duration::from_secs(request.timeout_seconds.saturating_add(30)),
        );
        tokio::pin!(capture);
        let deadline = tokio::time::sleep(Duration::from_secs(request.timeout_seconds));
        tokio::pin!(deadline);
        let mut terminal_kind = None;
        let captured = tokio::select! {
            value = &mut capture => value,
            _ = cancel.changed() => {
                terminal_kind = Some(ExecutionFailureKind::Cancelled);
                let _ = bridge.call("concept.stop", &run_id, json!({"attempt_id":attempt_id,"reason":"cancelled"}), Duration::from_secs(10)).await;
                tokio::time::timeout(Duration::from_secs(15), &mut capture).await
                    .map_err(|_| ExecutionFailure { kind: ExecutionFailureKind::Cancelled, message: "job cancelled; browser capture did not settle".into() })?
            },
            _ = &mut deadline => {
                terminal_kind = Some(ExecutionFailureKind::TimedOut);
                let _ = bridge.call("concept.stop", &run_id, json!({"attempt_id":attempt_id,"reason":"response-timeout"}), Duration::from_secs(10)).await;
                tokio::time::timeout(Duration::from_secs(15), &mut capture).await
                    .map_err(|_| ExecutionFailure { kind: ExecutionFailureKind::TimedOut, message: format!("job exceeded {} seconds", request.timeout_seconds) })?
            }
        }
        .map_err(browser_failure)?;
        let capture: BrowserCapture =
            serde_json::from_value(captured).map_err(|_| ExecutionFailure {
                kind: ExecutionFailureKind::Failed,
                message: "browser-capture-invalid: the browser returned an invalid response".into(),
            })?;
        tokio::fs::write(&result_path, &capture.markdown)
            .await
            .map_err(io_failure)?;
        bridge
            .call(
                "concept.close",
                &run_id,
                json!({"attempt_id":attempt_id,"durable_capture":true}),
                Duration::from_secs(10),
            )
            .await
            .map_err(browser_failure)?;
        if let Some(kind) = terminal_kind {
            return Err(ExecutionFailure {
                kind,
                message: if kind == ExecutionFailureKind::Cancelled {
                    "job cancelled; partial browser output was captured".into()
                } else {
                    format!(
                        "job exceeded {} seconds; partial browser output was captured",
                        request.timeout_seconds
                    )
                },
            });
        }
        if !capture.complete {
            let code = capture
                .error_code
                .unwrap_or_else(|| "browser-capture-incomplete".into());
            return Err(ExecutionFailure {
                kind: if code == "authentication-expired" {
                    ExecutionFailureKind::Authentication
                } else {
                    ExecutionFailureKind::Failed
                },
                message: code,
            });
        }
        Ok(ExecutionResult { exit_code: 0 })
    }
}

fn browser_failure(error: jailgun_orchestrator::concept::BrowserFailure) -> ExecutionFailure {
    ExecutionFailure {
        kind: if matches!(
            error.code.as_str(),
            "authentication-expired" | "account-mismatch"
        ) {
            ExecutionFailureKind::Authentication
        } else {
            ExecutionFailureKind::Failed
        },
        message: format!("{}: {}; {}", error.code, error.message, error.next_action),
    }
}

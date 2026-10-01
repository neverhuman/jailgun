use super::*;

impl WorkerService {
    pub async fn submit_job(&self, request: JobSubmit) -> Result<WorkerJob> {
        validate_job(&request)?;
        // Serialize acceptance, not execution. Identity observation and implicit
        // tab creation await before the key is recorded; concurrent retries must
        // not pass the key check twice or submit two owned provider turns.
        let _submission = self.inner.submissions.lock().await;
        let digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&request)?));
        {
            let state = self.inner.state.read().await;
            if let Some((stored_digest, job_id)) = state.idempotency.get(&request.idempotency_key) {
                if stored_digest != &digest {
                    return Err(action(
                        "idempotency-conflict",
                        "That idempotency key was already used with different job settings.",
                        "Reuse the original request or choose a new idempotency key.",
                    ));
                }
                return state
                    .jobs
                    .get(job_id)
                    .cloned()
                    .ok_or_else(|| not_found("job"));
            }
        }
        let tab_id = match request.tab_id.clone() {
            Some(tab_id) => {
                if request.input_object_id.is_some() {
                    return Err(action(
                        "input-object-with-existing-tab",
                        "An input object can be attached only while opening a new tab.",
                        "Open a tab with input_object_id, then submit the job to that tab.",
                    ));
                }
                tab_id
            }
            None => {
                self.open_tab(TabOpen {
                    tab_id: None,
                    account_id: request.account_id.clone(),
                    model: request.model.clone().unwrap_or_else(default_model),
                    reasoning_effort: request.reasoning_effort.unwrap_or_default(),
                    input_object_id: request.input_object_id.clone(),
                })
                .await?
                .tab_id
            }
        };
        let job_id = format!("job-{}", uuid::Uuid::new_v4().simple());
        let (account_id, model, reasoning_effort) = {
            let mut state = self.inner.state.write().await;
            let tab = state
                .tabs
                .get_mut(&tab_id)
                .ok_or_else(|| not_found("tab"))?;
            if tab.status != "open" || tab.active_job_id.is_some() {
                return Err(action(
                    "tab-busy",
                    "The selected tab is not idle and open.",
                    "Choose another tab or wait for the active job.",
                ));
            }
            let model = match &request.model {
                Some(model) => normalize_model(model)?,
                None => tab.model.clone(),
            };
            let effort = request.reasoning_effort.unwrap_or(tab.reasoning_effort);
            if let Some(requested_account) = &request.account_id {
                if requested_account != &tab.account_id {
                    return Err(action(
                        "account-tab-mismatch",
                        "The requested account does not own the selected tab.",
                        "Omit account_id or select a tab opened for that account.",
                    ));
                }
            }
            tab.model = model.clone();
            tab.reasoning_effort = effort;
            tab.active_job_id = Some(job_id.clone());
            (tab.account_id.clone(), model, effort)
        };
        let job = WorkerJob {
            job_id: job_id.clone(),
            tab_id: tab_id.clone(),
            account_id,
            status: "queued".into(),
            model,
            reasoning_effort,
            timeout_seconds: request.timeout_seconds,
            error_reporting: request.error_reporting,
            output_object_id: None,
            exit_code: None,
            error: None,
            created_ms: now_ms(),
            started_ms: None,
            finished_ms: None,
        };
        {
            let mut state = self.inner.state.write().await;
            state
                .idempotency
                .insert(request.idempotency_key.clone(), (digest, job_id.clone()));
            state.jobs.insert(job_id.clone(), job.clone());
        }
        let (cancel_tx, cancel_rx) = watch::channel(false);
        self.inner
            .cancellations
            .lock()
            .await
            .insert(job_id.clone(), cancel_tx);
        let worker = self.clone();
        tokio::spawn(async move {
            worker.run_job(job_id, request.prompt, cancel_rx).await;
        });
        Ok(job)
    }

    pub async fn job(&self, job_id: &str) -> Result<WorkerJob> {
        self.inner
            .state
            .read()
            .await
            .jobs
            .get(job_id)
            .cloned()
            .ok_or_else(|| not_found("job"))
    }

    pub async fn cancel_job(&self, request: JobRef) -> Result<WorkerJob> {
        let job = self.job(&request.job_id).await?;
        if is_terminal(&job.status) {
            return Ok(job);
        }
        if let Some(cancel) = self.inner.cancellations.lock().await.get(&request.job_id) {
            let _ = cancel.send(true);
        }
        let mut state = self.inner.state.write().await;
        let job = state
            .jobs
            .get_mut(&request.job_id)
            .ok_or_else(|| not_found("job"))?;
        job.status = "cancelling".into();
        Ok(job.clone())
    }

    async fn run_job(&self, job_id: String, prompt: String, cancel: watch::Receiver<bool>) {
        let (tab_id, account_id, request) = {
            let mut state = self.inner.state.write().await;
            let (tab_id, account_id, model, reasoning_effort, timeout_seconds, error_reporting) = {
                let Some(job) = state.jobs.get_mut(&job_id) else {
                    return;
                };
                job.status = "running".into();
                job.started_ms = Some(now_ms());
                (
                    job.tab_id.clone(),
                    job.account_id.clone(),
                    job.model.clone(),
                    job.reasoning_effort,
                    job.timeout_seconds,
                    job.error_reporting,
                )
            };
            let input_archive_path = state
                .tabs
                .get(&tab_id)
                .and_then(|tab| tab.input_object_id.as_deref())
                .map(|object_id| self.object_path(object_id));
            let request = ExecutionRequest {
                job_id: job_id.clone(),
                tab_id: tab_id.clone(),
                account_id: account_id.clone(),
                workspace: self.inner.root.join("tabs").join(&tab_id),
                log_dir: self.inner.root.join("jobs").join(&job_id),
                input_archive_path,
                prompt,
                model,
                reasoning_effort,
                timeout_seconds,
                error_reporting,
            };
            (tab_id, account_id, request)
        };
        let result = match tokio::time::timeout(
            Duration::from_secs(request.timeout_seconds.saturating_add(20)),
            self.inner.executor.execute(request.clone(), cancel),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(ExecutionFailure {
                kind: ExecutionFailureKind::TimedOut,
                message: format!("job exceeded {} seconds", request.timeout_seconds),
            }),
        };
        let output = if result.is_ok() {
            self.package_output(&job_id, &request.workspace).await.ok()
        } else {
            None
        };
        let authentication_failed = matches!(
            &result,
            Err(ExecutionFailure {
                kind: ExecutionFailureKind::Authentication,
                ..
            })
        );
        let mut state = self.inner.state.write().await;
        if let Some(tab) = state.tabs.get_mut(&tab_id) {
            if tab.active_job_id.as_deref() == Some(&job_id) {
                tab.active_job_id = None;
            }
        }
        if let Some(job) = state.jobs.get_mut(&job_id) {
            job.finished_ms = Some(now_ms());
            match result {
                Ok(result) => {
                    job.status = "completed".into();
                    job.exit_code = Some(result.exit_code);
                    job.output_object_id = output.as_ref().map(|object| object.object_id.clone());
                    if output.is_none() {
                        job.status = "failed".into();
                        job.error = Some("output archive could not be created".into());
                    }
                }
                Err(error) => {
                    job.status = match error.kind {
                        ExecutionFailureKind::Cancelled => "cancelled",
                        ExecutionFailureKind::TimedOut => "timed-out",
                        ExecutionFailureKind::Authentication => "failed",
                        ExecutionFailureKind::Failed => "failed",
                    }
                    .into();
                    if job.error_reporting != ErrorReporting::Off {
                        job.error = Some(error.message);
                    }
                }
            }
        }
        if let Some(object) = output {
            state.objects.insert(object.object_id.clone(), object);
        }
        let account = if authentication_failed {
            state.accounts.get_mut(&account_id).map(|account| {
                account.status = WorkerAccountStatus::LoginRequired;
                account.last_checked_ms = Some(now_ms());
                account.clone()
            })
        } else {
            None
        };
        drop(state);
        if let Some(account) = account.filter(|_| self.inner.account_store.is_none()) {
            let _ = persist_account(&self.inner.root, &account);
        }
        self.inner.cancellations.lock().await.remove(&job_id);
    }

    async fn package_output(&self, job_id: &str, workspace: &Path) -> Result<WorkerObject> {
        let object_id = format!("object-{}", uuid::Uuid::new_v4().simple());
        let path = self.object_path(&object_id);
        let workspace = workspace.to_path_buf();
        let output = path.clone();
        tokio::task::spawn_blocking(move || build_archive(&workspace, &output))
            .await
            .map_err(|_| {
                action(
                    "archive-task-failed",
                    "Output packaging stopped unexpectedly.",
                    "Inspect daemon logs and retry the job.",
                )
            })??;
        let size_bytes = path.metadata()?.len();
        let object = WorkerObject {
            object_id,
            name: format!("{job_id}.tar.gz"),
            kind: "output".into(),
            media_type: "application/gzip".into(),
            sha256: sha256_file(&path)?,
            size_bytes,
            created_ms: now_ms(),
        };
        persist_object(&self.inner.root, &object)?;
        Ok(object)
    }
}

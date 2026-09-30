use super::*;

struct MockExecutor;

#[async_trait]
impl WorkerExecutor for MockExecutor {
    fn name(&self) -> &'static str {
        "mock"
    }

    async fn auth_status(&self, codex_home: PathBuf) -> WorkerAccountStatus {
        if codex_home
            .components()
            .any(|component| component.as_os_str() == "expired")
        {
            WorkerAccountStatus::LoginRequired
        } else {
            WorkerAccountStatus::Ready
        }
    }

    async fn execute(
        &self,
        request: ExecutionRequest,
        _cancel: watch::Receiver<bool>,
    ) -> std::result::Result<ExecutionResult, ExecutionFailure> {
        std::fs::write(
            request.workspace.join("jailgun-result.md"),
            format!("# Mock result\n\n{}", request.prompt),
        )
        .map_err(io_failure)?;
        Ok(ExecutionResult { exit_code: 0 })
    }
}

struct BlockingExecutor;

#[async_trait]
impl WorkerExecutor for BlockingExecutor {
    fn name(&self) -> &'static str {
        "blocking-mock"
    }

    async fn execute(
        &self,
        _request: ExecutionRequest,
        mut cancel: watch::Receiver<bool>,
    ) -> std::result::Result<ExecutionResult, ExecutionFailure> {
        let _ = cancel.changed().await;
        Err(ExecutionFailure {
            kind: ExecutionFailureKind::Cancelled,
            message: "mock cancellation observed".into(),
        })
    }
}

struct SlowExecutor;

#[async_trait]
impl WorkerExecutor for SlowExecutor {
    fn name(&self) -> &'static str {
        "slow-mock"
    }

    async fn execute(
        &self,
        _request: ExecutionRequest,
        _cancel: watch::Receiver<bool>,
    ) -> std::result::Result<ExecutionResult, ExecutionFailure> {
        std::future::pending().await
    }
}

fn service(temp: &tempfile::TempDir) -> WorkerService {
    WorkerService::new(temp.path(), Arc::new(MockExecutor)).unwrap()
}

async fn register_default(service: &WorkerService) {
    let account = service
        .register_account(AccountRegister {
            account_id: "primary".into(),
            label: Some("Primary test account".into()),
        })
        .await
        .unwrap();
    assert_eq!(account.status, WorkerAccountStatus::Ready);
}

async fn wait_terminal(service: &WorkerService, job_id: &str) -> WorkerJob {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let job = service.job(job_id).await.unwrap();
            if is_terminal(&job.status) {
                return job;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn mock_job_uses_alias_effort_and_returns_verified_tar() {
    let temp = tempfile::tempdir().unwrap();
    let service = service(&temp);
    register_default(&service).await;
    let tab = service
        .open_tab(TabOpen {
            tab_id: Some("dream".into()),
            account_id: Some("primary".into()),
            model: "sol".into(),
            reasoning_effort: ReasoningEffort::Xhigh,
            input_object_id: None,
        })
        .await
        .unwrap();
    assert_eq!(tab.model, "gpt-5.6-sol");
    let request = JobSubmit {
        prompt: "Build the fixture".into(),
        tab_id: Some(tab.tab_id),
        account_id: None,
        model: Some("astra".into()),
        reasoning_effort: Some(ReasoningEffort::Ultra),
        timeout_seconds: 60,
        error_reporting: ErrorReporting::Detailed,
        input_object_id: None,
        idempotency_key: "fixture-1".into(),
    };
    let accepted = service.submit_job(request.clone()).await.unwrap();
    assert_eq!(
        service.submit_job(request).await.unwrap().job_id,
        accepted.job_id
    );
    let job = wait_terminal(&service, &accepted.job_id).await;
    assert_eq!(job.status, "completed");
    assert_eq!(job.model, "gpt-6-astra");
    assert_eq!(job.reasoning_effort, ReasoningEffort::Ultra);
    let object = service
        .object(job.output_object_id.as_deref().unwrap())
        .await
        .unwrap();
    assert_eq!(
        sha256_file(&service.object_path(&object.object_id)).unwrap(),
        object.sha256
    );
    jailgun_core::validate_tar_gz(service.object_path(&object.object_id), false).unwrap();
}

#[tokio::test]
async fn chunked_objects_round_trip_and_extract_safely() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("hello.txt"), "hello").unwrap();
    let archive = temp.path().join("source.tar.gz");
    build_archive(&source, &archive).unwrap();
    let bytes = std::fs::read(&archive).unwrap();
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let service = service(&temp);
    register_default(&service).await;
    let midpoint = bytes.len() / 2;
    let first = service
        .put_object(ObjectPut {
            upload_id: None,
            name: "source.tar.gz".into(),
            offset: 0,
            data_base64: BASE64.encode(&bytes[..midpoint]),
            final_chunk: false,
            total_bytes: bytes.len() as u64,
            sha256: digest.clone(),
        })
        .await
        .unwrap();
    let final_chunk = service
        .put_object(ObjectPut {
            upload_id: Some(first.upload_id),
            name: "source.tar.gz".into(),
            offset: midpoint as u64,
            data_base64: BASE64.encode(&bytes[midpoint..]),
            final_chunk: true,
            total_bytes: bytes.len() as u64,
            sha256: digest,
        })
        .await
        .unwrap();
    let object = final_chunk.object.unwrap();
    let tab = service
        .open_tab(TabOpen {
            tab_id: Some("from-object".into()),
            account_id: Some("primary".into()),
            model: "astra".into(),
            reasoning_effort: ReasoningEffort::Medium,
            input_object_id: Some(object.object_id.clone()),
        })
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(
            service
                .inner
                .root
                .join("tabs")
                .join(tab.tab_id)
                .join("hello.txt")
        )
        .unwrap(),
        "hello"
    );
    let chunk = service
        .get_object(ObjectGet {
            object_id: object.object_id,
            offset: 0,
            limit: MAX_CHUNK_BYTES as u32,
        })
        .await
        .unwrap();
    assert_eq!(BASE64.decode(chunk.data_base64).unwrap(), bytes);
    assert!(chunk.eof);
}

#[tokio::test]
async fn busy_tabs_and_idempotency_conflicts_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let service = service(&temp);
    register_default(&service).await;
    let request = JobSubmit {
        prompt: "first".into(),
        tab_id: None,
        account_id: None,
        model: None,
        reasoning_effort: None,
        timeout_seconds: 60,
        error_reporting: ErrorReporting::Off,
        input_object_id: None,
        idempotency_key: "same".into(),
    };
    let accepted = service.submit_job(request.clone()).await.unwrap();
    let mut changed = request;
    changed.prompt = "different".into();
    assert_eq!(
        service.submit_job(changed).await.unwrap_err().code(),
        "idempotency-conflict"
    );
    let _ = wait_terminal(&service, &accepted.job_id).await;
    let closed = service
        .close_tab(TabRef {
            tab_id: accepted.tab_id,
            force: false,
        })
        .await
        .unwrap();
    assert_eq!(closed.status, "closed");
}

#[tokio::test]
async fn mock_cancellation_and_timeout_reach_distinct_terminal_states() {
    let cancelled_root = tempfile::tempdir().unwrap();
    let cancelled = WorkerService::new(cancelled_root.path(), Arc::new(BlockingExecutor)).unwrap();
    register_default(&cancelled).await;
    let request = JobSubmit {
        prompt: "wait".into(),
        tab_id: None,
        account_id: None,
        model: Some("sol".into()),
        reasoning_effort: Some(ReasoningEffort::Medium),
        timeout_seconds: 30,
        error_reporting: ErrorReporting::Summary,
        input_object_id: None,
        idempotency_key: "cancel-fixture".into(),
    };
    let job = cancelled.submit_job(request).await.unwrap();
    cancelled
        .cancel_job(JobRef {
            job_id: job.job_id.clone(),
        })
        .await
        .unwrap();
    let job = wait_terminal(&cancelled, &job.job_id).await;
    assert_eq!(job.status, "cancelled");
    assert_eq!(job.error.as_deref(), Some("mock cancellation observed"));

    let timeout_root = tempfile::tempdir().unwrap();
    let timeout = WorkerService::new(timeout_root.path(), Arc::new(SlowExecutor)).unwrap();
    register_default(&timeout).await;
    let job = timeout
        .submit_job(JobSubmit {
            prompt: "wait forever".into(),
            tab_id: None,
            account_id: None,
            model: None,
            reasoning_effort: None,
            timeout_seconds: 1,
            error_reporting: ErrorReporting::Off,
            input_object_id: None,
            idempotency_key: "timeout-fixture".into(),
        })
        .await
        .unwrap();
    let job = wait_terminal(&timeout, &job.job_id).await;
    assert_eq!(job.status, "timed-out");
    assert!(job.error.is_none());
}

#[tokio::test]
async fn multiple_accounts_require_explicit_routing_and_stay_private() {
    let temp = tempfile::tempdir().unwrap();
    let service = service(&temp);
    for account_id in ["first", "second"] {
        service
            .register_account(AccountRegister {
                account_id: account_id.into(),
                label: None,
            })
            .await
            .unwrap();
    }
    let expired = service
        .register_account(AccountRegister {
            account_id: "expired".into(),
            label: Some("Needs login".into()),
        })
        .await
        .unwrap();
    assert_eq!(expired.status, WorkerAccountStatus::LoginRequired);
    let accounts = service.accounts(true).await.unwrap();
    assert_eq!(accounts.counts.total, 3);
    assert_eq!(accounts.counts.ready, 2);
    assert_eq!(accounts.counts.login_required, 1);
    let error = service
        .open_tab(TabOpen {
            tab_id: Some("expired-tab".into()),
            account_id: Some("expired".into()),
            model: "astra".into(),
            reasoning_effort: ReasoningEffort::Medium,
            input_object_id: None,
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), "account-login-required");
    let error = service
        .open_tab(TabOpen {
            tab_id: Some("ambiguous".into()),
            account_id: None,
            model: "astra".into(),
            reasoning_effort: ReasoningEffort::High,
            input_object_id: None,
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), "account-required");
    let tab = service
        .open_tab(TabOpen {
            tab_id: Some("routed".into()),
            account_id: Some("second".into()),
            model: "sol".into(),
            reasoning_effort: ReasoningEffort::Xhigh,
            input_object_id: None,
        })
        .await
        .unwrap();
    assert_eq!(tab.account_id, "second");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(service.account_root("second").join("account.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}

#[test]
fn authentication_failures_are_detected_without_returning_credentials() {
    let temp = tempfile::tempdir().unwrap();
    let stderr = temp.path().join("stderr.log");
    std::fs::write(&stderr, "request failed: HTTP 401 unauthorized\n").unwrap();
    assert!(stderr_indicates_auth_failure(&stderr));
    std::fs::write(&stderr, "ordinary model failure\n").unwrap();
    assert!(!stderr_indicates_auth_failure(&stderr));
}

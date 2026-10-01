//! Network retries must not create two implicit tabs or submit two browser jobs.
use async_trait::async_trait;
use jailgun_server::{worker::*, WorkerService};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct YieldingExecutor(AtomicUsize);

#[async_trait]
impl WorkerExecutor for YieldingExecutor {
    fn name(&self) -> &'static str {
        "yielding-mock"
    }

    async fn auth_status(&self, _: &str) -> WorkerAccountStatus {
        tokio::task::yield_now().await;
        WorkerAccountStatus::Ready
    }

    async fn execute(
        &self,
        request: ExecutionRequest,
        _: tokio::sync::watch::Receiver<bool>,
    ) -> std::result::Result<ExecutionResult, ExecutionFailure> {
        self.0.fetch_add(1, Ordering::SeqCst);
        std::fs::write(
            request.workspace.join("jailgun-result.md"),
            "Mock retry proof",
        )
        .unwrap();
        Ok(ExecutionResult { exit_code: 0 })
    }
}

#[tokio::test]
async fn concurrent_identical_retries_accept_and_execute_exactly_one_job() {
    let directory = tempfile::tempdir().unwrap();
    let executor = Arc::new(YieldingExecutor(AtomicUsize::new(0)));
    let worker = WorkerService::new(directory.path(), executor.clone()).unwrap();
    worker
        .register_account(AccountRegister {
            account_id: "primary".into(),
            label: None,
        })
        .await
        .unwrap();
    let request = JobSubmit {
        prompt: "Synthetic retry proof".into(),
        tab_id: None,
        account_id: Some("primary".into()),
        model: None,
        reasoning_effort: None,
        timeout_seconds: 60,
        error_reporting: ErrorReporting::Detailed,
        input_object_id: None,
        idempotency_key: "concurrent-retry".into(),
    };
    let (first, second) = tokio::join!(
        worker.submit_job(request.clone()),
        worker.submit_job(request.clone())
    );
    let first = first.unwrap();
    assert_eq!(
        first.job_id,
        second.unwrap().job_id,
        "concurrent retry created another job"
    );
    assert_eq!(
        worker.tabs().await.tabs.len(),
        1,
        "concurrent retry created another tab"
    );
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let job = worker.job(&first.job_id).await.unwrap();
            if job.status == "completed" {
                break;
            }
            assert_ne!(job.status, "failed");
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        executor.0.load(Ordering::SeqCst),
        1,
        "concurrent retry executed another job"
    );
    let mut conflict = request;
    conflict.prompt = "Different synthetic request".into();
    assert_eq!(
        worker.submit_job(conflict).await.unwrap_err().code(),
        "idempotency-conflict"
    );
}

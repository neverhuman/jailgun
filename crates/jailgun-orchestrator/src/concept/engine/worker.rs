use super::*;

pub(super) async fn execute(
    store: Store,
    bridge: ConceptBridge,
    lease: Lease,
    clock: Arc<dyn WorkflowClock>,
) -> anyhow::Result<()> {
    let run = store.run(lease.run_id.clone()).await?;
    let prompt = match store.prompt(lease.clone()).await {
        Ok(prompt) => prompt,
        Err(error) => {
            store
                .fail_attempt(
                    lease,
                    error.code().into(),
                    FailureDisposition::Terminal,
                    clock.now_ms(),
                )
                .await?;
            return Ok(());
        }
    };
    let preparation=bridge.call("concept.prepare",&lease.run_id,json!({"attempt_id":lease.attempt_id,"prompt":prompt,"model":run.configuration.model,"timeout_ms":run.configuration.response_timeout_ms}),Duration::from_secs(90)).await;
    if let Err(error) = preparation {
        record_browser_failure(&store, &lease, &error, false, clock.now_ms()).await?;
        let _ = bridge
            .call(
                "concept.close",
                &lease.run_id,
                json!({"attempt_id":lease.attempt_id,"confirmed_not_submitted":true}),
                Duration::from_secs(10),
            )
            .await;
        return Ok(());
    }
    loop {
        let current = store.run(lease.run_id.clone()).await?;
        if current.status == "cancelled" {
            bridge
                .call(
                    "concept.close",
                    &lease.run_id,
                    json!({"attempt_id":lease.attempt_id,"confirmed_not_submitted":true}),
                    Duration::from_secs(10),
                )
                .await?;
            store.acknowledge_cancel(lease, clock.now_ms()).await?;
            return Ok(());
        }
        if clock.now_ms() >= current.deadline_ms || current.submissions >= current.submission_limit
        {
            store
                .fail_attempt(
                    lease.clone(),
                    if clock.now_ms() >= current.deadline_ms {
                        "run-deadline"
                    } else {
                        "submission-budget-exhausted"
                    }
                    .into(),
                    FailureDisposition::Terminal,
                    clock.now_ms(),
                )
                .await?;
            bridge
                .call(
                    "concept.close",
                    &lease.run_id,
                    json!({"attempt_id":lease.attempt_id,"confirmed_not_submitted":true}),
                    Duration::from_secs(10),
                )
                .await?;
            return Ok(());
        }
        match store.begin_submission(lease.clone(), clock.now_ms()).await {
            Ok(()) => break,
            Err(error) if matches!(error.code(), "submission-paced" | "invalid-transition") => {
                tokio::time::sleep(Duration::from_millis(100)).await
            }
            Err(error) => return Err(error.into()),
        }
    }
    let accepted = bridge
        .call(
            "concept.submit",
            &lease.run_id,
            json!({"attempt_id":lease.attempt_id}),
            Duration::from_secs(60),
        )
        .await;
    let turn: AcceptedTurn = match accepted {
        Ok(value) => match serde_json::from_value(value) {
            Ok(turn) => turn,
            Err(_) => {
                store
                    .fail_attempt(
                        lease,
                        "adapter-invalid-acceptance".into(),
                        FailureDisposition::Uncertain,
                        clock.now_ms(),
                    )
                    .await?;
                return Ok(());
            }
        },
        Err(error) => {
            record_browser_failure(&store, &lease, &error, true, clock.now_ms()).await?;
            return Ok(());
        }
    };
    store.accepted(lease.clone(), turn, clock.now_ms()).await?;
    capture(store, bridge, lease, clock).await
}

pub(super) async fn capture(
    store: Store,
    bridge: ConceptBridge,
    lease: Lease,
    clock: Arc<dyn WorkflowClock>,
) -> anyhow::Result<()> {
    let result = bridge
        .call(
            "concept.capture",
            &lease.run_id,
            json!({"attempt_id":lease.attempt_id}),
            Duration::from_secs(31 * 60),
        )
        .await;
    let capture: BrowserCapture = match result {
        Ok(value) => serde_json::from_value(value)?,
        Err(error) => {
            record_browser_failure(&store, &lease, &error, true, clock.now_ms()).await?;
            return Ok(());
        }
    };
    if !capture.complete {
        if capture.error_code.as_deref() == Some("authentication-expired") {
            store
                .account_expired(lease.account_id.clone(), clock.now_ms())
                .await?;
        }
        if capture.error_code.as_deref() == Some("account-rate-limited") {
            store
                .rate_limited(
                    lease.account_id.clone(),
                    capture.retry_at_ms,
                    clock.now_ms(),
                )
                .await?;
        }
        if let Err(error) = bridge
            .call(
                "concept.stop",
                &lease.run_id,
                json!({"attempt_id":lease.attempt_id}),
                Duration::from_secs(10),
            )
            .await
        {
            let mut partial = capture.into_capture(&lease.stage);
            partial.error_code.get_or_insert(error.code);
            store
                .preserve_partial(lease, partial, clock.now_ms())
                .await?;
            return Ok(());
        }
    }
    store
        .capture(
            lease.clone(),
            capture.into_capture(&lease.stage),
            clock.now_ms(),
        )
        .await?;
    bridge
        .call(
            "concept.close",
            &lease.run_id,
            json!({"attempt_id":lease.attempt_id,"durable_capture":true}),
            Duration::from_secs(10),
        )
        .await?;
    Ok(())
}

async fn record_browser_failure(
    store: &Store,
    lease: &Lease,
    error: &BrowserFailure,
    submission_started: bool,
    now: i64,
) -> anyhow::Result<()> {
    if matches!(
        error.code.as_str(),
        "authentication-expired" | "account-mismatch"
    ) {
        store.account_expired(lease.account_id.clone(), now).await?;
    }
    if error.code == "account-rate-limited" || error.rate_limited {
        store
            .rate_limited(lease.account_id.clone(), error.retry_at_ms, now)
            .await?;
    }
    let disposition = if submission_started && error.code != "submission-not-accepted" {
        FailureDisposition::Uncertain
    } else {
        FailureDisposition::DefinitelyNotAccepted
    };
    store
        .fail_attempt(lease.clone(), error.code.clone(), disposition, now)
        .await?;
    if !submission_started
        && matches!(
            error.code.as_str(),
            "model-unavailable" | "model-mismatch" | "adapter-model-unobservable"
        )
    {
        store
            .suspend(lease.run_id.clone(), error.code.clone(), now)
            .await?;
    }
    Ok(())
}

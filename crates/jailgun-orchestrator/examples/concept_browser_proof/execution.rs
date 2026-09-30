use super::*;

pub(super) async fn check(
    store: &Store,
    engine: &mut ConceptEngine,
    clock: &FixtureClock,
    run_id: &str,
    mode: &str,
) -> Result<serde_json::Value> {
    engine.tick().await?;
    let deadline = Instant::now() + Duration::from_secs(15);
    if mode == "rate-limit-uncertain" {
        while store.run(run_id.into()).await?.status != "reconciliation-required" {
            anyhow::ensure!(
                Instant::now() < deadline,
                "uncertain submission was not preserved"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        engine.tick().await?;
        let run = store.run(run_id.into()).await?;
        let attempts = store.attempts(run_id.into()).await?;
        let accounts = store.accounts().await?;
        anyhow::ensure!(
            run.submissions == 1 && attempts.len() == 1 && attempts[0].state == "uncertain",
            "uncertain submission retried"
        );
        anyhow::ensure!(
            attempts[0].error_code.as_deref() == Some("submission-uncertain"),
            "limit replaced acceptance uncertainty"
        );
        anyhow::ensure!(
            accounts[0].active == 1
                && accounts[0].readiness == "cooldown"
                && accounts[0].cooldown_until_ms == 2_000_000,
            "uncertain attempt lost its reservation or provider cooldown"
        );
        return Ok(
            json!({"status":"pass","scope":"rust-scheduler-real-chrome-synthetic-provider","live_provider_verified":false,"execution_control":mode,"submission_count":1,"reconciliation_required":true}),
        );
    }
    let accepted = loop {
        if let Some(attempt) = store
            .attempts(run_id.into())
            .await?
            .into_iter()
            .find(|a| a.accepted_ms.is_some())
        {
            break attempt;
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "execution proof never accepted a prompt"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    if mode == "rate-limit" {
        while store.accounts().await?[0].readiness == "ready" {
            anyhow::ensure!(
                Instant::now() < deadline,
                "rate limit did not reach Rust scheduling"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    } else {
        let run = store.run(run_id.into()).await?;
        if mode == "response-timeout" {
            // Pausing new submissions must not disable the accepted response deadline.
            store.pause(run_id.into(), clock.now_ms()).await?;
        }
        // Exercise cancellation between adapter polls with a deliberately endless stream.
        tokio::time::sleep(Duration::from_millis(600)).await;
        let limit = if mode == "run-deadline" {
            run.deadline_ms
        } else {
            accepted.accepted_ms.unwrap() + run.configuration.response_timeout_ms
        };
        clock
            .1
            .fetch_add(limit + 1 - clock.now_ms(), Ordering::SeqCst);
    }
    loop {
        engine.tick().await?;
        if engine.active_count() == 0 {
            break;
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "active conversation did not stop at its execution limit"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let run = store.run(run_id.into()).await?;
    let code = if mode == "rate-limit" {
        "account-rate-limited"
    } else {
        mode
    };
    let attempts = store.attempts(run_id.into()).await?;
    anyhow::ensure!(
        attempts.len() == 1 && run.submissions == 1,
        "execution control submitted extra prompts"
    );
    anyhow::ensure!(
        attempts[0].error_code.as_deref() == Some(code),
        "original execution error missing: {:?}",
        attempts[0]
    );
    anyhow::ensure!(
        attempts[0].state == "partial" && !run.artifacts.is_empty(),
        "interrupted text was not retained as partial"
    );
    anyhow::ensure!(
        run.artifacts.iter().all(|a| a.completion == "partial"),
        "an interrupted response became complete"
    );
    anyhow::ensure!(
        store.active_leases().await?.is_empty(),
        "confirmed stop retained capacity"
    );
    anyhow::ensure!(
        store.result(run_id.into()).await.is_err(),
        "interrupted work produced a completed result"
    );
    if mode == "rate-limit" {
        let account = &store.accounts().await?[0];
        anyhow::ensure!(
            account.readiness == "cooldown" && account.cooldown_until_ms == 2_000_000,
            "provider retry time was not respected"
        );
        anyhow::ensure!(
            store.claim(1_999_999, 0).await?.is_none(),
            "account resumed before provider retry time"
        );
    }
    Ok(
        json!({"status":"pass","scope":"rust-scheduler-real-chrome-synthetic-provider","live_provider_verified":false,"execution_control":mode,"submission_count":run.submissions,"error_code":code,"artifacts":run.artifacts}),
    )
}

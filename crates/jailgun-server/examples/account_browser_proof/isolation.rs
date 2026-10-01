use super::*;
use jailgun_workflow::model::{ConfirmAccount, ModelSelection};
use std::path::Path;

pub(super) async fn verify(
    url: &str,
    service: &AccountSupervisor,
    store: &Store,
    options: &BrowserRuntime,
    clock: Arc<FixtureClock>,
    output: &Path,
) -> Result<()> {
    let client = reqwest::Client::builder().no_proxy().build()?;
    for id in ["other", "synthetic"] {
        admin(&client, options, json!({"isolatedAccounts":true,"account":{"id":format!("provider-{id}"),"email":format!("{id}@example.invalid")}})).await?;
        client
            .post(format!("{url}/api/accounts/connect"))
            .bearer_auth(TOKEN)
            .json(&json!({"id":id,"email":format!("{id}@example.invalid")}))
            .send()
            .await?
            .error_for_status()?;
        let anonymous = wait(store, id, "waiting-for-user").await?;
        anyhow::ensure!(
            anonymous.observation.is_none(),
            "anonymous identity accepted"
        );
        service.fixture_login(id).await?;
        confirm(service, store, id).await?;
    }
    let metadata = store.account_metadata().await?;
    anyhow::ensure!(metadata.len() == 2, "expected two accounts");
    anyhow::ensure!(
        metadata[0].profile_dir != metadata[1].profile_dir
            && metadata[0].cdp_port != metadata[1].cdp_port,
        "accounts share browser allocation"
    );
    for account in &metadata {
        anyhow::ensure!(
            account.provider_account_id.as_deref()
                == Some(format!("provider-{}", account.id).as_str()),
            "wrong provider binding"
        );
    }
    // Expire one identity while the other still owns its separate persistent cookie.
    admin(
        &client,
        options,
        json!({"accountId":"provider-other","expired":true}),
    )
    .await?;
    wait(store, "other", "expired").await?;
    wait(store, "synthetic", "ready").await?;
    let worker = jailgun_server::WorkerService::browser(output, service.clone())?;
    let expired = worker.accounts(true).await?;
    anyhow::ensure!(
        expired.counts.ready == 1,
        "refresh accepted an expired browser"
    );
    wait(store, "other", "expired").await?;
    admin(
        &client,
        options,
        json!({"accountId":"provider-other","expired":false}),
    )
    .await?;
    let refreshed = worker.accounts(true).await?;
    anyhow::ensure!(
        refreshed.counts.ready == 2,
        "worker refresh did not recheck live identity"
    );
    wait(store, "other", "ready").await?;
    admin(
        &client,
        options,
        json!({"accountId":"provider-other","expired":true}),
    )
    .await?;
    wait(store, "other", "expired").await?;
    service.reconnect("other".into()).await?;
    wait(store, "other", "waiting-for-user").await?;
    service.cancel_login("other".into()).await?;
    wait(store, "other", "cancelled").await?;
    worker.accounts(true).await?;
    wait(store, "other", "cancelled").await?;

    let workflow = client::WorkflowClient::connect(url, "http", output).await?;
    let accepted = workflow.submit(json!({"concept":"Synthetic account-isolation concept","account_id":"synthetic","candidate_count":5,"idempotency_key":"two-account-isolation"})).await?;
    let id = accepted["run_id"].as_str().context("missing run")?;
    let deadline = Instant::now() + Duration::from_secs(100);
    loop {
        service.tick().await?;
        let run = workflow.status(id).await?;
        if run.status == "completed" {
            anyhow::ensure!(run.submissions == 9, "unexpected submissions");
            break;
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "other-account cancellation blocked ready work: {}",
            run.status
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    anyhow::ensure!(
        workflow.result(id).await?.status == "completed",
        "missing complete result"
    );
    workflow.close().await?;
    wait(store, "other", "cancelled").await?;
    admin(&client, options, json!({"account":{"id":"provider-other","email":"other@example.invalid"},"accountId":"provider-other","expired":false,"models":["Fixture One","First Account Model"]})).await?;
    service.reconnect("other".into()).await?;
    // Existing cookies are reused: no second synthetic login is performed.
    wait(store, "other", "ready").await?;
    service.shutdown().await;
    let restarted = AccountSupervisor::with_clock(store.clone(), options.clone(), clock).await?;
    let result = async {
        restarted.restore_accounts().await?;
        for account in &metadata {
            wait(store, &account.id, "ready").await?;
        }
        let after = store.account_metadata().await?;
        for before in &metadata {
            let after = after
                .iter()
                .find(|a| a.id == before.id)
                .context("lost account")?;
            anyhow::ensure!(
                after.profile_dir == before.profile_dir
                    && after.cdp_port == before.cdp_port
                    && after.provider_account_id == before.provider_account_id,
                "restart changed account allocation"
            );
        }
        anyhow::ensure!(
            store.result(id.into()).await?.status == "completed",
            "restart lost results"
        );
        Ok::<_, anyhow::Error>(())
    }
    .await;
    restarted.shutdown().await;
    result?;
    std::fs::write(
        output.join("evidence.json"),
        serde_json::to_vec_pretty(
            &json!({"status":"pass","checks":["two-persistent-identities","distinct-profiles-and-ports","one-account-expiry","fresh-probe-recovers-authentication-without-login","refresh-preserves-cancelled-login","one-account-login-cancellation","other-account-five-stage-workflow","two-account-cookie-reuse-after-browser-restart","retained-allocation-and-results"],"candidate_count":5,"submissions":9,"synthetic_provider":true,"live_provider_verified":false}),
        )?,
    )?;
    Ok(())
}

async fn admin(
    client: &reqwest::Client,
    options: &BrowserRuntime,
    body: serde_json::Value,
) -> Result<()> {
    client
        .post(format!("{}/admin/concepts", options.provider_url))
        .json(&body)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}
async fn confirm(service: &AccountSupervisor, store: &Store, id: &str) -> Result<()> {
    let observed = wait(store, id, "verifying")
        .await?
        .observation
        .context("missing identity")?;
    anyhow::ensure!(
        observed.identity.id == format!("provider-{id}"),
        "wrong observed identity"
    );
    service
        .confirm(
            id.into(),
            ConfirmAccount {
                identity: observed.identity,
                model: ModelSelection::Current,
            },
        )
        .await?;
    wait(store, id, "ready").await?;
    Ok(())
}
async fn wait(store: &Store, id: &str, lifecycle: &str) -> Result<AccountSession> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(session) = store
            .account_sessions()
            .await?
            .into_iter()
            .find(|s| s.account_id == id && s.lifecycle == lifecycle)
        {
            return Ok(session);
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "account {id} did not reach {lifecycle}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

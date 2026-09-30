use anyhow::{Context, Result};
use jailgun_orchestrator::concept::AccountSupervisor;
use jailgun_workflow::Store;
use serde_json::json;

pub(super) async fn verify(
    client: &reqwest::Client,
    url: &str,
    provider: &str,
    service: &AccountSupervisor,
    store: &Store,
    completed_run: &str,
) -> Result<()> {
    let before = store.run(completed_run.into()).await?;
    let metadata = store.account_metadata().await?;
    // Start an account probe, then reproduce expiry reported by another owned
    // conversation before reconnect navigates the account page.
    delay_probe(client, provider).await?;
    client
        .post(format!("{provider}/admin/concepts"))
        .json(&json!({"expired":true,"models":["Fixture One","Fixture Two","Fixture Three"]}))
        .send()
        .await?
        .error_for_status()?;
    store
        .account_expired(
            "synthetic".into(),
            (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64,
        )
        .await?;
    client
        .post(format!("{url}/api/accounts/synthetic/reconnect"))
        .bearer_auth(super::TOKEN)
        .send()
        .await?
        .error_for_status()?;
    super::wait_session(client, url, "waiting-for-user").await?;
    delay_probe(client, provider).await?;
    client
        .post(format!("{url}/api/accounts/synthetic/cancel-login"))
        .bearer_auth(super::TOKEN)
        .send()
        .await?
        .error_for_status()?;
    super::wait_session(client, url, "cancelled").await?;
    tokio::time::sleep(std::time::Duration::from_secs(6)).await;
    anyhow::ensure!(
        store.account_sessions().await?[0].lifecycle == "cancelled",
        "a probe overwrote cancelled login"
    );
    client
        .post(format!("{url}/api/accounts/synthetic/reconnect"))
        .bearer_auth(super::TOKEN)
        .send()
        .await?
        .error_for_status()?;
    super::wait_session(client, url, "waiting-for-user").await?;
    service.fixture_login("synthetic").await?;
    let ready = super::wait_session(client, url, "ready").await?;
    let observation = ready
        .observation
        .context("missing reauthenticated identity")?;
    anyhow::ensure!(
        observation
            .available_models
            .contains(&"Fixture Three".into()),
        "reconnect retained a stale model list: {:?}",
        observation.available_models
    );
    client
        .post(format!("{url}/api/accounts/synthetic/confirm"))
        .bearer_auth(super::TOKEN)
        .json(&json!({"identity":observation.identity,"model":{"mode":"specific","name":"Fixture Three"}}))
        .send()
        .await?
        .error_for_status()?;
    // An explicit reconnect also refreshes a currently ready account. Repeated
    // setup still reuses the existing ready session without navigating it.
    client
        .post(format!("{provider}/admin/concepts"))
        .json(&json!({"models":["Fixture One","Fixture Two","Fixture Three","Fixture Four"]}))
        .send()
        .await?
        .error_for_status()?;
    client
        .post(format!("{url}/api/accounts/synthetic/reconnect"))
        .bearer_auth(super::TOKEN)
        .send()
        .await?
        .error_for_status()?;
    let refreshed = super::wait_session(client, url, "ready").await?;
    anyhow::ensure!(
        refreshed
            .observation
            .context("missing ready observation")?
            .available_models
            .contains(&"Fixture Four".into()),
        "explicit reconnect skipped fresh verification"
    );
    let after_metadata = store.account_metadata().await?;
    anyhow::ensure!(
        metadata[0].id == after_metadata[0].id
            && metadata[0].profile_dir == after_metadata[0].profile_dir
            && metadata[0].cdp_port == after_metadata[0].cdp_port,
        "reconnect changed the managed account allocation"
    );
    let after = store.run(completed_run.into()).await?;
    anyhow::ensure!(
        after.status == "completed" && before.submissions == after.submissions,
        "reconnect resubmitted completed work"
    );
    anyhow::ensure!(
        store.result(completed_run.into()).await?.status == "completed",
        "completed results lost during reconnect"
    );
    Ok(())
}

async fn delay_probe(client: &reqwest::Client, provider: &str) -> Result<()> {
    client
        .post(format!("{provider}/admin/concepts"))
        .json(&json!({"nextAuthDelayMs":2000}))
        .send()
        .await?
        .error_for_status()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let state: serde_json::Value = client
            .get(format!("{provider}/admin/concepts"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if state["pendingAuth"] == 1 {
            return Ok(());
        }
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "account probe did not enter delay"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

//! Exercises HTTP onboarding and HTTP/MCP concept workflows with real sandboxed Chrome.
#[path = "account_browser_proof/cli.rs"]
mod cli;
#[path = "account_browser_proof/client.rs"]
mod client;
#[path = "account_browser_proof/dashboard.rs"]
mod dashboard;
#[path = "account_browser_proof/isolation.rs"]
mod isolation;
#[path = "account_browser_proof/reconnect.rs"]
mod reconnect;
use anyhow::{Context, Result};
use jailgun_core::JailgunConfig;
use jailgun_orchestrator::concept::{AccountSupervisor, BrowserRuntime, WorkflowClock};
use jailgun_server::{api_router, router_with_static, AppState};
use jailgun_workflow::{model::AccountSession, Store};
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

struct FixtureClock {
    start: Instant,
    epoch: i64,
}
impl WorkflowClock for FixtureClock {
    fn now_ms(&self) -> i64 {
        self.epoch + (self.start.elapsed().as_millis() * 100) as i64
    }
}
const TOKEN: &str = "synthetic-http-proof-operator-credential-0001";

#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    anyhow::ensure!(
        (3..=5).contains(&args.len()) && args[0].starts_with("http://127.0.0.1:"),
        "expected loopback fixture, private output and Chrome"
    );
    let output = PathBuf::from(&args[1]).canonicalize()?;
    let options = BrowserRuntime {
        node: "node".into(),
        bridge: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/chrome-bridge/bin/concept-bridge.mjs")
            .canonicalize()?,
        chrome: Some(args[2].clone().into()),
        headless: args.get(4).is_none_or(|s| s != "viewer"),
        server_browser: args.get(4).is_some_and(|s| s == "viewer"),
        provider_url: args[0].clone(),
        environment: BTreeMap::new(),
    };
    let clock = Arc::new(FixtureClock {
        start: Instant::now(),
        epoch: (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64,
    });
    let count = args
        .get(3)
        .map(|s| s.parse::<u8>())
        .transpose()?
        .unwrap_or(5);
    let mode = args.get(4).map(String::as_str).unwrap_or("http");
    anyhow::ensure!([5, 10].contains(&count), "proof count must be five or ten");
    run_proof(options, clock, output, count, mode).await
}

async fn run_proof(
    options: BrowserRuntime,
    clock: Arc<FixtureClock>,
    output: PathBuf,
    count: u8,
    mode: &str,
) -> Result<()> {
    let store = Store::open(output.join("runtime"))?;
    let service =
        AccountSupervisor::with_clock(store.clone(), options.clone(), clock.clone()).await?;
    let (state, _) = AppState::live(JailgunConfig::default(), output.join("receipts"), 64);
    let state = state
        .with_ingest_token(Some(TOKEN.into()))
        .with_account_supervisor(service.clone());
    let pairing = state.dashboard_sessions.issue_pairing_code();
    let router = if matches!(mode, "dashboard" | "viewer") {
        router_with_static(
            state,
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/dashboard/dist"),
        )
    } else {
        api_router(state)
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}", listener.local_addr()?);
    let http = tokio::spawn(async move { axum::serve(listener, router).await });
    if mode == "two-accounts" {
        let result = isolation::verify(&url, &service, &store, &options, clock, &output).await;
        service.shutdown().await;
        http.abort();
        let _ = http.await;
        store.close().await?;
        return result;
    }
    if matches!(mode, "dashboard" | "viewer") {
        let result = dashboard::drive(&url, &pairing, &service, &store).await;
        service.shutdown().await;
        http.abort();
        let _ = http.await;
        store.close().await?;
        return result;
    }
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{url}/api/accounts/connect"))
        .bearer_auth(TOKEN)
        .json(&json!({"email":"synthetic@example.invalid","id":"synthetic"}))
        .send()
        .await?;
    anyhow::ensure!(
        response.status().is_success(),
        "connect failed: {}",
        response.text().await?
    );
    let account = wait_session(&client, &url, "waiting-for-user").await?;
    anyhow::ensure!(
        account.observation.is_none(),
        "anonymous account was verified"
    );
    // No credential or verification code is sent through the HTTP API.
    service.fixture_login("synthetic").await?;
    let detected = wait_session(&client, &url, "verifying").await?;
    let observation = detected.observation.context("missing observed identity")?;
    anyhow::ensure!(
        observation.identity.id == "synthetic-chatgpt-account",
        "unexpected identity"
    );
    let response=client.post(format!("{url}/api/accounts/synthetic/confirm")).bearer_auth(TOKEN).json(&json!({"identity":observation.identity,"model":{"mode":"specific","name":"Fixture Two"}})).send().await?;
    anyhow::ensure!(
        response.status().is_success(),
        "confirm failed: {}",
        response.text().await?
    );
    wait_session(&client, &url, "ready").await?;
    let transport = if mode == "reconnect" { "http" } else { mode };
    let workflow = client::WorkflowClient::connect(&url, transport, &output).await?;
    let accepted=workflow.submit(json!({"concept":"A synthetic neighborhood lending library","account_id":"synthetic","candidate_count":count,"idempotency_key":"account-workflow-proof"})).await?;
    let id = accepted["run_id"].as_str().context("missing run ID")?;
    let deadline = Instant::now() + Duration::from_secs(100);
    let completed = loop {
        service.tick().await?;
        let run = workflow.status(id).await?;
        if run.status == "completed" {
            break run;
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "workflow did not complete: {} {:?}",
            run.status,
            run.pause_reason
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    anyhow::ensure!(
        completed.submissions == u16::from(count) + 4,
        "unexpected submission count"
    );
    let result = workflow.result(id).await?;
    let final_artifact = result
        .artifacts
        .iter()
        .find(|a| a.name == "final.md")
        .context("missing final")?;
    let bytes = client
        .get(format!(
            "{url}/api/runs/{id}/artifacts/{}",
            final_artifact.id
        ))
        .bearer_auth(TOKEN)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    anyhow::ensure!(!bytes.is_empty(), "empty final artifact");
    workflow.verify_results(&completed, &bytes).await?;
    workflow.close().await?;
    if mode == "reconnect" {
        if let Err(error) =
            reconnect::verify(&client, &url, &options.provider_url, &service, &store, id).await
        {
            service.shutdown().await;
            http.abort();
            let _ = http.await;
            store.close().await?;
            return Err(error);
        }
    }
    service.shutdown().await;
    http.abort();
    let _ = http.await;
    store.close().await?;
    let reopened = Store::open(output.join("runtime"))?;
    let restart_ms = (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64;
    let restarted = AccountSupervisor::with_clock(reopened.clone(), options, clock).await?;
    restarted.restore_accounts().await?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if reopened.account_sessions().await?.iter().any(|s| {
            s.lifecycle == "ready" && s.observed_ms.is_some_and(|observed| observed >= restart_ms)
        }) {
            break;
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "profile login was not reused after restart"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    anyhow::ensure!(
        reopened
            .accounts()
            .await?
            .iter()
            .any(|a| a.id == "synthetic" && a.readiness == "ready"),
        "restarted account did not pass fresh authentication"
    );
    anyhow::ensure!(
        reopened.result(id.to_string()).await?.status == "completed",
        "completed results lost"
    );
    restarted.shutdown().await;
    reopened.close().await?;
    std::fs::write(
        output.join("evidence.json"),
        serde_json::to_vec_pretty(
            &json!({"status":"pass","checks":["anonymous-rejected","manual-login","identity-confirmation","explicit-model","five-stage-workflow","final-download","restart-cookie-reuse","durable-results"],"reconnect_model_refresh":mode=="reconnect","submissions":u16::from(count)+4,"candidate_count":count,"transport":mode,"synthetic_provider":true,"live_provider_verified":false}),
        )?,
    )?;
    Ok(())
}

async fn wait_session(client: &reqwest::Client, url: &str, wanted: &str) -> Result<AccountSession> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let sessions: Vec<AccountSession> = client
            .get(format!("{url}/api/accounts/sessions"))
            .bearer_auth(TOKEN)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if let Some(session) = sessions
            .into_iter()
            .find(|s| s.account_id == "synthetic" && s.lifecycle == wanted)
        {
            return Ok(session);
        }
        anyhow::ensure!(Instant::now() < deadline, "account never reached {wanted}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

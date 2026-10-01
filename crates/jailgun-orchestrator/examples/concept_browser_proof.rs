//! Real browser workflow proof against the explicit loopback synthetic provider.
use anyhow::{Context, Result};
use jailgun_core::{BrowserAccount, BrowserAccountStatus, BrowserProfileRegistry};
use jailgun_orchestrator::{
    bridge::BridgeSpawnConfig,
    concept::{ConceptBridge, ConceptEngine, WorkflowClock},
};
use jailgun_workflow::{
    model::{default_criteria, ConceptRequest, ModelSelection},
    Store,
};
use serde_json::json;
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[path = "concept_browser_proof/execution.rs"]
mod execution;

struct FixtureClock(Instant, AtomicI64);
impl WorkflowClock for FixtureClock {
    fn now_ms(&self) -> i64 {
        (self.0.elapsed().as_millis() * 100) as i64 + self.1.load(Ordering::SeqCst)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    anyhow::ensure!(
        (4..=5).contains(&args.len()),
        "expected fixture URL, private output directory, Chrome executable and candidate count"
    );
    let origin = &args[0];
    anyhow::ensure!(
        origin.starts_with("http://127.0.0.1:"),
        "this proof only accepts the loopback synthetic provider"
    );
    let output = PathBuf::from(&args[1]).canonicalize()?;
    let count: u16 = args[3].parse()?;
    anyhow::ensure!([5, 10].contains(&count), "proof count must be five or ten");
    let runtime = output.join("runtime");
    let profile = output.join("profile");
    let cdp_listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let cdp_port = cdp_listener.local_addr()?.port();
    drop(cdp_listener);
    let mut store = Store::open(&runtime)?;
    let account = BrowserAccount {
        id: "synthetic-account".into(),
        email_hint: "synthetic@example.invalid".into(),
        profile_dir: profile.clone(),
        state_dir: output.join("state"),
        downloads_dir: output.join("downloads"),
        cdp_port,
        max_tabs: 10,
        status: BrowserAccountStatus::Ready,
        last_verified_at: Some("synthetic".into()),
        provider_account_id: Some("synthetic-chatgpt-account".into()),
    };
    let registry = output.join("accounts.json");
    BrowserProfileRegistry {
        version: 1,
        accounts: vec![account],
    }
    .save(&registry)?;
    store.import_registry(registry, 0).await?;
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/chrome-bridge/bin/concept-bridge.mjs")
        .canonicalize()?;
    let mut bridge = browser(&script, &profile, origin, &args[2], cdp_port).await?;
    bridge
        .call(
            "concept.fixture-login",
            "account",
            json!({}),
            Duration::from_secs(10),
        )
        .await?;
    let auth = bridge
        .call(
            "concept.auth-status",
            "account",
            json!({}),
            Duration::from_secs(10),
        )
        .await?;
    anyhow::ensure!(
        auth["identity"]["id"] == "synthetic-chatgpt-account",
        "wrong fixture identity"
    );
    let clock = Arc::new(FixtureClock(Instant::now(), AtomicI64::new(0)));
    let mut engine = ConceptEngine::with_clock(
        store.clone(),
        HashMap::from([("synthetic-account".into(), bridge.clone())]),
        clock.clone(),
    );
    let isolation = args.get(4).is_some_and(|mode| mode == "isolation");
    let victim = if isolation {
        let victim = store
            .submit(
                ConceptRequest {
                    concept: "A synthetic concept whose work will be cancelled".into(),
                    account_id: "synthetic-account".into(),
                    candidate_count: 5,
                    constraints: String::new(),
                    criteria: default_criteria(),
                    idempotency_key: "cancel-victim".into(),
                },
                ModelSelection::Current,
                clock.now_ms(),
            )
            .await?;
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            engine.tick().await?;
            if store
                .attempts(victim.id.clone())
                .await?
                .iter()
                .any(|attempt| attempt.state == "accepted")
            {
                break;
            }
            anyhow::ensure!(
                Instant::now() < deadline,
                "cancel victim never accepted a prompt"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        store.pause(victim.id.clone(), clock.now_ms()).await?;
        Some(victim.id)
    } else {
        None
    };
    let run = store
        .submit(
            ConceptRequest {
                concept: "A synthetic neighborhood lending library".into(),
                account_id: "synthetic-account".into(),
                candidate_count: count,
                constraints: "A small operating budget".into(),
                criteria: default_criteria(),
                idempotency_key: format!("synthetic-{count}"),
            },
            ModelSelection::Specific {
                name: "Fixture Two".into(),
            },
            clock.now_ms(),
        )
        .await?;
    if let Some(mode) = args.get(4).filter(|mode| {
        [
            "run-deadline",
            "response-timeout",
            "rate-limit",
            "rate-limit-uncertain",
        ]
        .contains(&mode.as_str())
    }) {
        let evidence = execution::check(&store, &mut engine, &clock, &run.id, mode).await?;
        engine.quiesce().await;
        bridge.shutdown().await?;
        store.close().await?;
        let reopened = Store::open(&runtime)?;
        anyhow::ensure!(
            reopened.run(run.id).await?.submissions == 1,
            "restart reset the charged submission budget"
        );
        reopened.close().await?;
        std::fs::write(
            output.join("evidence.json"),
            serde_json::to_vec_pretty(&evidence)?,
        )?;
        println!("{}", serde_json::to_string(&evidence)?);
        return Ok(());
    }
    let recovery = args.get(4).is_some_and(|mode| mode == "recovery");
    if recovery {
        engine.tick().await?;
        let accepted_deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if store
                .attempts(run.id.clone())
                .await?
                .iter()
                .any(|attempt| attempt.state == "accepted")
            {
                break;
            }
            anyhow::ensure!(
                Instant::now() < accepted_deadline,
                "acceptance timed out before restart proof"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        anyhow::ensure!(
            store.run(run.id.clone()).await?.submissions == 1,
            "restart proof must interrupt exactly one accepted submission"
        );
        drop(engine);
        bridge.shutdown().await?;
        store.close().await?;
        store = Store::open(&runtime)?;
        bridge = browser(&script, &profile, origin, &args[2], cdp_port).await?;
        let reused = bridge
            .call(
                "concept.auth-status",
                "account",
                json!({}),
                Duration::from_secs(10),
            )
            .await?;
        anyhow::ensure!(
            reused["identity"]["id"] == "synthetic-chatgpt-account",
            "login was not reused after browser restart"
        );
        engine = ConceptEngine::with_clock(
            store.clone(),
            HashMap::from([("synthetic-account".into(), bridge.clone())]),
            clock.clone(),
        );
        engine.recover().await?;
        anyhow::ensure!(
            store.run(run.id.clone()).await?.submissions == 1,
            "recovery repeated a submission"
        );
    }
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut maximum_active = 0;
    let mut cancelled = false;
    loop {
        anyhow::ensure!(
            Instant::now() < deadline,
            "browser workflow proof timed out"
        );
        engine.tick().await?;
        if let Some(victim_id) = victim.as_ref().filter(|_| !cancelled) {
            if store
                .attempts(run.id.clone())
                .await?
                .iter()
                .any(|attempt| attempt.state == "accepted")
            {
                let attempt = store
                    .attempts(victim_id.clone())
                    .await?
                    .into_iter()
                    .find(|attempt| attempt.state == "accepted")
                    .context("victim completed before simultaneous cancellation")?;
                let wrong_owner = bridge
                    .call(
                        "concept.stop",
                        &run.id,
                        json!({"attempt_id":attempt.id}),
                        Duration::from_secs(10),
                    )
                    .await;
                anyhow::ensure!(
                    wrong_owner.is_err_and(|error| error.code == "ownership-mismatch"),
                    "cross-run browser mutation was not rejected"
                );
                store.cancel(victim_id.clone(), clock.now_ms()).await?;
                store.cancel(victim_id.clone(), clock.now_ms()).await?;
                cancelled = true;
            }
        }
        maximum_active = maximum_active.max(store.accounts().await?[0].active);
        let current = store.run(run.id.clone()).await?;
        if current.status == "completed" && engine.active_count() == 0 {
            break;
        }
        anyhow::ensure!(
            ![
                "failed",
                "paused",
                "reconciliation-required",
                "waiting-for-auth"
            ]
            .contains(&current.status.as_str()),
            "workflow stopped: {} {:?}; tasks {:?}",
            current.status,
            current.pause_reason,
            current
                .tasks
                .iter()
                .map(|task| (&task.stage, task.position, &task.status, &task.error_code))
                .collect::<Vec<_>>()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let complete = store.run(run.id.clone()).await?;
    anyhow::ensure!(
        complete.submissions == count + 4,
        "unexpected retry/submission count"
    );
    anyhow::ensure!(
        complete.tasks.iter().all(|task| task.status == "completed"),
        "unfinished stages"
    );
    anyhow::ensure!(
        recovery || isolation || maximum_active >= count,
        "did not exercise all candidate conversations concurrently: {maximum_active}"
    );
    if let Some(victim_id) = victim {
        let stopped = store.run(victim_id.clone()).await?;
        anyhow::ensure!(
            cancelled && stopped.status == "cancelled",
            "cancellation did not persist"
        );
        anyhow::ensure!(
            store
                .active_leases()
                .await?
                .iter()
                .all(|lease| lease.run_id != victim_id),
            "cancelled conversations retained capacity after confirmed stop"
        );
        anyhow::ensure!(
            stopped
                .artifacts
                .iter()
                .all(|artifact| artifact.completion == "partial"),
            "cancelled streaming text became complete"
        );
    }
    let events = store.events(run.id.clone(), 0).await?;
    let starts = events
        .iter()
        .filter(|event| event.kind == "submission-started")
        .map(|event| event.created_ms)
        .collect::<Vec<_>>();
    anyhow::ensure!(
        starts.windows(2).all(|pair| pair[1] - pair[0] >= 30_000),
        "account pacing was violated"
    );
    let manifest = store.result(run.id.clone()).await?;
    let final_file = complete
        .artifacts
        .iter()
        .find(|artifact| artifact.name == "final.md")
        .context("missing final concept")?;
    let (_, final_text) = store
        .artifact(run.id.clone(), final_file.id.clone())
        .await?;
    anyhow::ensure!(
        String::from_utf8(final_text)?.contains("```rust"),
        "Markdown code fence missing"
    );
    let attempts = store.attempts(run.id.clone()).await?;
    anyhow::ensure!(
        attempts.iter().all(|attempt| attempt.user_turn_id.is_some()
            && attempt.observed_model.as_deref() == Some("Fixture Two")),
        "missing accepted turn/model binding"
    );
    let evidence = json!({"status":"pass","scope":"rust-scheduler-real-chrome-synthetic-provider","live_provider_verified":false,"restart_recovery":recovery,"cancellation_isolation":isolation,"candidate_count":count,"submission_count":complete.submissions,"maximum_active":maximum_active,"clock":"100x fixture clock; same persisted pacing policy","sqlite_version":store.sqlite_version().await?,"artifacts":manifest.artifacts});
    bridge.shutdown().await?;
    drop(engine);
    store.close().await?;
    let reopened = Store::open(&runtime)?;
    anyhow::ensure!(
        reopened.result(run.id).await?.status == "completed",
        "result did not survive restart"
    );
    reopened.close().await?;
    std::fs::write(
        output.join("evidence.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;
    println!(
        "{}",
        serde_json::to_string(
            &json!({"status":"pass","candidates":count,"maximum_active":maximum_active,"submissions":count+4})
        )?
    );
    Ok(())
}

async fn browser(
    script: &Path,
    profile: &Path,
    origin: &str,
    executable: &str,
    cdp_port: u16,
) -> Result<ConceptBridge> {
    ConceptBridge::spawn(BridgeSpawnConfig {command:vec!["node".into(),script.to_string_lossy().into()],env:BTreeMap::new()},
        json!({"profile_dir":profile,"base_url":origin,"identity":{"id":"synthetic-chatgpt-account","email":"synthetic@example.invalid"},"headless":true,"executable":executable,"cdp_port":cdp_port})).await
}

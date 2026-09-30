use super::{BrowserFailure, ConceptBridge};
use jailgun_workflow::{
    model::{AcceptedTurn, Lease},
    BrowserCapture, FailureDisposition, Store,
};
use serde_json::json;
use std::{collections::HashMap, sync::Arc, time::Duration};

pub trait WorkflowClock: Send + Sync {
    fn now_ms(&self) -> i64;
}
pub struct SystemClock;
impl WorkflowClock for SystemClock {
    fn now_ms(&self) -> i64 {
        (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64
    }
}

struct Active {
    lease: Lease,
    work: tokio::task::JoinHandle<anyhow::Result<()>>,
    stop_requested: bool,
}

pub struct ConceptEngine {
    store: Store,
    accounts: HashMap<String, ConceptBridge>,
    active: HashMap<String, Active>,
    clock: Arc<dyn WorkflowClock>,
    recovered: bool,
}

impl ConceptEngine {
    pub fn new(store: Store, accounts: HashMap<String, ConceptBridge>) -> Self {
        Self::with_clock(store, accounts, Arc::new(SystemClock))
    }

    /// Test harnesses may advance a deterministic clock against a loopback fixture.
    /// Production services always use SystemClock and the persisted pacing defaults.
    pub fn with_clock(
        store: Store,
        accounts: HashMap<String, ConceptBridge>,
        clock: Arc<dyn WorkflowClock>,
    ) -> Self {
        Self {
            store,
            accounts,
            active: HashMap::new(),
            clock,
            recovered: false,
        }
    }

    pub async fn tick(&mut self) -> anyhow::Result<()> {
        let finished = self
            .active
            .iter()
            .filter(|(_, active)| active.work.is_finished())
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in finished {
            if let Some(active) = self.active.remove(&id) {
                if !matches!(active.work.await, Ok(Ok(()))) {
                    tracing::error!(run_id=%active.lease.run_id, attempt_id=%active.lease.attempt_id, "concept worker stopped; preserving owned conversations for reconciliation");
                    if self
                        .store
                        .active_leases()
                        .await?
                        .iter()
                        .any(|lease| lease.attempt_id == active.lease.attempt_id)
                    {
                        self.store
                            .fail_attempt(
                                active.lease,
                                "worker-interrupted".into(),
                                FailureDisposition::Uncertain,
                                self.clock.now_ms(),
                            )
                            .await?;
                    }
                }
            }
        }
        for active in self.active.values_mut() {
            if active.stop_requested {
                continue;
            }
            let run = self.store.run(active.lease.run_id.clone()).await?;
            let now = self.clock.now_ms();
            let reason = if run.status == "cancelled" {
                Some("cancelled")
            } else if now >= run.deadline_ms {
                Some("run-deadline")
            } else {
                self.store
                    .attempts(run.id)
                    .await?
                    .iter()
                    .find(|attempt| attempt.id == active.lease.attempt_id)
                    .and_then(|attempt| attempt.accepted_ms)
                    .filter(|accepted| {
                        now >= accepted.saturating_add(run.configuration.response_timeout_ms)
                    })
                    .map(|_| "response-timeout")
            };
            if let Some(reason) = reason {
                if let Some(bridge) = self.accounts.get(&active.lease.account_id) {
                    // Stop is scoped to the reservation's run and attempt. Never restart a shared browser.
                    let result = bridge
                        .call(
                            "concept.stop",
                            &active.lease.run_id,
                            json!({"attempt_id":active.lease.attempt_id,"reason":reason}),
                            Duration::from_secs(10),
                        )
                        .await;
                    if result.is_ok() {
                        active.stop_requested = true;
                    }
                }
            }
        }
        loop {
            let jitter = (uuid::Uuid::new_v4().as_u128() % 5001) as i64;
            let Some(lease) = self.store.claim(self.clock.now_ms(), jitter).await? else {
                break;
            };
            let Some(bridge) = self.accounts.get(&lease.account_id).cloned() else {
                self.store
                    .account_expired(lease.account_id.clone(), self.clock.now_ms())
                    .await?;
                self.store
                    .fail_attempt(
                        lease,
                        "browser-unavailable".into(),
                        FailureDisposition::DefinitelyNotAccepted,
                        self.clock.now_ms(),
                    )
                    .await?;
                continue;
            };
            let store = self.store.clone();
            let clock = self.clock.clone();
            let owned = lease.clone();
            let work = tokio::spawn(async move { execute(store, bridge, owned, clock).await });
            self.active.insert(
                lease.attempt_id.clone(),
                Active {
                    lease,
                    work,
                    stop_requested: false,
                },
            );
        }
        Ok(())
    }

    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    pub fn attach_account(&mut self, id: String, bridge: ConceptBridge) {
        self.accounts.insert(id, bridge);
    }

    pub async fn quiesce(&mut self) {
        // Keep durable acceptance/uncertainty records for restart reconciliation.
        for (_, active) in self.active.drain() {
            active.work.abort();
            let _ = active.work.await;
        }
    }

    /// Reattach only accepted attempts whose conversation and user turn are persisted.
    pub async fn recover(&mut self) -> anyhow::Result<()> {
        let mut reattached: HashMap<String, Vec<String>> = HashMap::new();
        let leases = if self.recovered {
            self.store.active_leases().await?
        } else {
            let leases = self.store.recover(self.clock.now_ms()).await?;
            self.recovered = true;
            leases
        };
        for lease in leases {
            if self.active.contains_key(&lease.attempt_id) {
                continue;
            }
            let Some(bridge) = self.accounts.get(&lease.account_id).cloned() else {
                continue;
            };
            let attempts = self.store.attempts(lease.run_id.clone()).await?;
            let Some(attempt) = attempts
                .iter()
                .find(|attempt| attempt.id == lease.attempt_id && attempt.state == "accepted")
            else {
                continue;
            };
            let (
                Some(conversation_id),
                Some(conversation_url),
                Some(user_turn_id),
                Some(observed_model),
            ) = (
                attempt.conversation_id.clone(),
                attempt.conversation_url.clone(),
                attempt.user_turn_id.clone(),
                attempt.observed_model.clone(),
            )
            else {
                continue;
            };
            let run = self.store.run(lease.run_id.clone()).await?;
            let remaining = (attempt.accepted_ms.unwrap_or(0)
                + run.configuration.response_timeout_ms
                - self.clock.now_ms())
            .max(1);
            let turn = AcceptedTurn {
                conversation_id,
                conversation_url,
                user_turn_id,
                observed_model,
            };
            if bridge.call("concept.reattach",&lease.run_id,json!({"attempt_id":lease.attempt_id,"turn":turn,"model":run.configuration.model,"timeout_ms":remaining}),Duration::from_secs(60)).await.is_err() {continue;}
            self.store
                .reattached(lease.clone(), turn.conversation_id, self.clock.now_ms())
                .await?;
            reattached
                .entry(lease.run_id.clone())
                .or_default()
                .push(lease.attempt_id.clone());
            let store = self.store.clone();
            let clock = self.clock.clone();
            let owned = lease.clone();
            let work = tokio::spawn(async move { capture(store, bridge, owned, clock).await });
            self.active.insert(
                lease.attempt_id.clone(),
                Active {
                    lease,
                    work,
                    stop_requested: false,
                },
            );
        }
        for (run_id, attempt_ids) in reattached {
            self.store
                .resume_reattached(run_id, attempt_ids, self.clock.now_ms())
                .await?;
        }
        Ok(())
    }
}

impl Drop for ConceptEngine {
    fn drop(&mut self) {
        for active in self.active.values() {
            active.work.abort();
        }
    }
}

async fn execute(
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

async fn capture(
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

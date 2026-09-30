use super::{ConceptBridge, ConceptEngine, SystemClock, WorkflowClock};
use jailgun_core::BrowserAccount;
use jailgun_workflow::{model::ConfirmAccount, Error, Store};
use serde_json::json;
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::sync::Mutex;
mod observe;
mod viewer;

#[derive(Clone)]
pub struct BrowserRuntime {
    pub node: PathBuf,
    pub bridge: PathBuf,
    pub chrome: Option<PathBuf>,
    pub headless: bool,
    pub server_browser: bool,
    /// ChatGPT in production; only an explicit loopback fixture is accepted otherwise.
    pub provider_url: String,
    pub environment: BTreeMap<String, String>,
}

#[derive(Clone)]
pub struct AccountSupervisor {
    pub store: Store,
    runtime: BrowserRuntime,
    engine: Arc<Mutex<ConceptEngine>>,
    bridges: Arc<Mutex<HashMap<String, ConceptBridge>>>,
    watchers: Arc<Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
    displays: Arc<Mutex<HashMap<String, Arc<super::display::ManagedDisplay>>>>,
    controls: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}

impl AccountSupervisor {
    pub async fn new(store: Store, runtime: BrowserRuntime) -> anyhow::Result<Self> {
        Self::with_clock(store, runtime, Arc::new(SystemClock)).await
    }

    /// Injectable time is reserved for deterministic loopback browser proofs.
    pub async fn with_clock(
        store: Store,
        runtime: BrowserRuntime,
        clock: Arc<dyn WorkflowClock>,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            !runtime.server_browser || (!runtime.headless && cfg!(target_os = "linux")),
            "server-browser requires Linux and a visible managed browser"
        );
        let mut engine = ConceptEngine::with_clock(store.clone(), HashMap::new(), clock);
        engine.recover().await?;
        Ok(Self {
            store,
            runtime,
            engine: Arc::new(Mutex::new(engine)),
            bridges: Arc::new(Mutex::new(HashMap::new())),
            watchers: Arc::new(Mutex::new(HashMap::new())),
            displays: Arc::new(Mutex::new(HashMap::new())),
            controls: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn restore_accounts(&self) -> anyhow::Result<()> {
        for account in self.store.account_metadata().await? {
            // Authentication is always checked again before recovered work is scheduled.
            self.store
                .account_expired(account.id.clone(), now())
                .await?;
            if self
                .store
                .account_sessions()
                .await?
                .iter()
                .any(|s| s.account_id == account.id && s.lifecycle == "cancelled")
            {
                continue;
            }
            self.start(account).await?;
        }
        Ok(())
    }

    pub async fn tick(&self) -> anyhow::Result<()> {
        self.engine.lock().await.tick().await
    }

    pub async fn start(&self, account: BrowserAccount) -> anyhow::Result<()> {
        self.start_login(account, false).await
    }

    async fn account_control(&self, id: &str) -> Arc<Mutex<()>> {
        self.controls
            .lock()
            .await
            .entry(id.into())
            .or_default()
            .clone()
    }

    async fn start_login(&self, account: BrowserAccount, reconnect: bool) -> anyhow::Result<()> {
        let control = self.account_control(&account.id).await;
        let _guard = control.lock().await;
        let id = account.id.clone();
        let session = self
            .store
            .account_sessions()
            .await?
            .into_iter()
            .find(|s| s.account_id == id)
            .ok_or_else(|| anyhow::anyhow!("account-not-found"))?;
        let active = self.bridges.lock().await.get(&id).cloned();
        let account_ready = self
            .store
            .accounts()
            .await?
            .iter()
            .any(|a| a.id == id && a.readiness == "ready");
        if !reconnect && session.lifecycle == "ready" && account_ready && active.is_some() {
            return Ok(());
        }
        if account_ready {
            self.store.account_expired(id.clone(), now()).await?;
        }
        self.store
            .login_state(id.clone(), "starting".into(), None, Some(now() + 900_000))
            .await?;
        let watching = self
            .watchers
            .lock()
            .await
            .get(&id)
            .is_some_and(|task| !task.is_finished());
        if watching {
            if let Some(bridge) = active {
                bridge
                    .call(
                        "concept.login-open",
                        "account",
                        json!({}),
                        Duration::from_secs(45),
                    )
                    .await?;
            }
            return Ok(());
        }
        let supervisor = self.clone();
        self.watchers.lock().await.insert(
            id,
            tokio::spawn(async move { supervisor.watch(account).await }),
        );
        Ok(())
    }

    pub async fn reconnect(&self, id: String) -> anyhow::Result<()> {
        let account = self
            .store
            .account_metadata()
            .await?
            .into_iter()
            .find(|a| a.id == id)
            .ok_or_else(|| anyhow::anyhow!("account-not-found"))?;
        self.start_login(account, true).await
    }

    pub async fn confirm(&self, id: String, request: ConfirmAccount) -> anyhow::Result<()> {
        let control = self.account_control(&id).await;
        let _guard = control.lock().await;
        let bridge = self
            .bridges
            .lock()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("browser-unavailable"))?;
        bridge
            .call(
                "concept.bind-identity",
                "account",
                json!({"identity":request.identity}),
                Duration::from_secs(15),
            )
            .await?;
        let mut engine = self.engine.lock().await;
        self.store
            .confirm_account(id.clone(), request, now())
            .await?;
        engine.attach_account(id, bridge);
        engine.recover().await?;
        Ok(())
    }

    pub async fn cancel_login(&self, id: String) -> anyhow::Result<()> {
        let control = self.account_control(&id).await;
        let _guard = control.lock().await;
        let session = self
            .store
            .account_sessions()
            .await?
            .into_iter()
            .find(|s| s.account_id == id)
            .ok_or_else(|| anyhow::anyhow!("account-not-found"))?;
        anyhow::ensure!(
            [
                "starting",
                "login-required",
                "waiting-for-user",
                "verifying",
                "cancelled"
            ]
            .contains(&session.lifecycle.as_str()),
            "login-not-active"
        );
        self.store
            .login_state(id.clone(), "cancelled".into(), None, None)
            .await?;
        self.store.account_expired(id, now()).await?;
        Ok(())
    }

    pub async fn shutdown(&self) {
        for (_, task) in self.watchers.lock().await.drain() {
            task.abort();
            let _ = task.await;
        }
        self.engine.lock().await.quiesce().await;
        for (_, bridge) in self.bridges.lock().await.drain() {
            let _ = bridge.shutdown().await;
        }
        for (_, display) in self.displays.lock().await.drain() {
            display.shutdown().await;
        }
    }

    /// Synthetic identity operation; the browser adapter rejects non-loopback providers.
    #[doc(hidden)]
    pub async fn fixture_login(&self, id: &str) -> anyhow::Result<()> {
        let bridge = self
            .bridges
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("browser-unavailable"))?;
        bridge
            .call(
                "concept.fixture-login",
                "account",
                json!({}),
                Duration::from_secs(10),
            )
            .await?;
        Ok(())
    }
}

fn now() -> i64 {
    SystemClock.now_ms()
}

pub fn service_error(error: anyhow::Error) -> Error {
    match error.downcast::<Error>() {
        Ok(error) => error,
        Err(_) => Error::action(
            "account-operation-failed",
            "The account browser operation could not complete.",
            "Inspect account status and reconnect through the operator dashboard.",
        ),
    }
}

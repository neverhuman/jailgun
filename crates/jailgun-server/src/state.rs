use std::{collections::HashMap, path::PathBuf, sync::Arc};

use jailgun_core::{
    BrowserProfileRegistry, EventKind, JailgunAgentRunSummary, JailgunConfig, JailgunEvent,
    RunSnapshot,
};
use jailgun_orchestrator::{AgentRunBackend, DefaultAgentRunBackend};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};

#[derive(Clone)]
pub struct AppState {
    pub daemon_control: Option<crate::DaemonControl>,
    pub account_supervisor: Option<jailgun_orchestrator::concept::AccountSupervisor>,
    pub workflow: Option<jailgun_workflow::Store>,
    pub worker: Option<crate::worker::WorkerService>,
    pub dashboard_sessions: Arc<crate::auth::DashboardSessions>,
    pub config: JailgunConfig,
    pub config_path: Option<PathBuf>,
    pub runs: Arc<RwLock<Vec<RunSnapshot>>>,
    pub agent_summaries: Arc<RwLock<HashMap<String, JailgunAgentRunSummary>>>,
    pub events: Arc<RwLock<Vec<JailgunEvent>>>,
    pub receipt_dir: PathBuf,
    pub browser_registry_path: PathBuf,
    pub browser_registry_lock: Arc<Mutex<()>>,
    pub browser_auth_sessions: Arc<RwLock<HashMap<String, BrowserAuthSession>>>,
    /// When `Some`, the WS endpoint subscribes to it and streams live events.
    /// When `None`, the WS endpoint replays `events` once and closes
    /// (fixture mode used by `jailgun fixture`).
    pub event_bus: Option<broadcast::Sender<JailgunEvent>>,
    /// Operator credential for all private endpoints (bearer or x-jailgun-token).
    /// With no credential, private endpoints fail closed with 503.
    pub ingest_token: Option<String>,
    pub agent_backend: Arc<dyn AgentRunBackend>,
}

#[derive(Clone)]
pub struct BrowserAuthSession {
    pub session_id: uuid::Uuid,
    pub commands_tx: mpsc::Sender<jailgun_orchestrator::bridge::Envelope<serde_json::Value>>,
}

impl AppState {
    pub fn fixture(config: JailgunConfig) -> Self {
        let run = RunSnapshot::fixture();
        let browser_registry_path =
            BrowserProfileRegistry::default_path_from_env(&config.browser.profile_registry_env);
        Self {
            daemon_control: None,
            account_supervisor: None,
            workflow: None,
            worker: None,
            dashboard_sessions: Arc::new(crate::auth::DashboardSessions::default()),
            config,
            config_path: None,
            runs: Arc::new(RwLock::new(vec![run.clone()])),
            agent_summaries: Arc::new(RwLock::new(HashMap::new())),
            events: Arc::new(RwLock::new(vec![
                JailgunEvent::new(&run.run_id, EventKind::RunStarted, "fixture run started"),
                JailgunEvent::new(
                    &run.run_id,
                    EventKind::TarDiscovered,
                    "tar candidate discovered",
                )
                .with_tab(1),
                JailgunEvent::new(
                    &run.run_id,
                    EventKind::RemoteSafety,
                    "remote safety state updated",
                ),
            ])),
            receipt_dir: PathBuf::from("receipts"),
            browser_registry_path,
            browser_registry_lock: Arc::new(Mutex::new(())),
            browser_auth_sessions: Arc::new(RwLock::new(HashMap::new())),
            event_bus: None,
            ingest_token: None,
            agent_backend: Arc::new(DefaultAgentRunBackend),
        }
    }

    /// Construct a live-bus AppState. Returns the state alongside one
    /// pre-subscribed receiver so the caller can drive a `fold_runs` task or
    /// archive events to disk without racing the first WS client.
    pub fn live(
        config: JailgunConfig,
        receipt_dir: PathBuf,
        capacity: usize,
    ) -> (Self, broadcast::Receiver<JailgunEvent>) {
        let (tx, rx) = broadcast::channel(capacity.max(64));
        let browser_registry_path =
            BrowserProfileRegistry::default_path_from_env(&config.browser.profile_registry_env);
        let state = Self {
            daemon_control: None,
            account_supervisor: None,
            workflow: None,
            worker: None,
            dashboard_sessions: Arc::new(crate::auth::DashboardSessions::default()),
            config,
            config_path: None,
            runs: Arc::new(RwLock::new(Vec::new())),
            agent_summaries: Arc::new(RwLock::new(HashMap::new())),
            events: Arc::new(RwLock::new(Vec::new())),
            receipt_dir,
            browser_registry_path,
            browser_registry_lock: Arc::new(Mutex::new(())),
            browser_auth_sessions: Arc::new(RwLock::new(HashMap::new())),
            event_bus: Some(tx),
            ingest_token: None,
            agent_backend: Arc::new(DefaultAgentRunBackend),
        };
        (state, rx)
    }

    pub fn with_ingest_token(mut self, token: Option<String>) -> Self {
        self.ingest_token = token;
        self.dashboard_sessions = Arc::new(crate::auth::DashboardSessions::default());
        self
    }

    pub fn with_daemon_control(mut self, control: crate::DaemonControl) -> Self {
        self.daemon_control = Some(control);
        self
    }

    pub fn with_workflow(mut self, store: jailgun_workflow::Store) -> Self {
        self.workflow = Some(store);
        self
    }

    pub fn with_worker(mut self, worker: crate::worker::WorkerService) -> Self {
        self.worker = Some(worker);
        self
    }

    pub fn with_account_supervisor(
        mut self,
        supervisor: jailgun_orchestrator::concept::AccountSupervisor,
    ) -> Self {
        self.workflow = Some(supervisor.store.clone());
        self.account_supervisor = Some(supervisor);
        self
    }

    pub fn with_config_path(mut self, path: Option<PathBuf>) -> Self {
        self.config_path = path;
        self
    }

    pub fn with_agent_backend(mut self, backend: Arc<dyn AgentRunBackend>) -> Self {
        self.agent_backend = backend;
        self
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct JailgunAgentRunAcceptedResponse {
    pub run_id: String,
    pub status: String,
    pub summary_json: String,
    pub events_jsonl: String,
    pub run_url: String,
    pub summary_url: String,
}

pub(crate) fn timestamp_now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

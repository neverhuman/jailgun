pub mod bus;

pub use bus::{BroadcastBus, EventBus, NoopBus, RecordingBus};

use std::{
    collections::HashMap,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::extract::ws::Message;
use axum::{
    extract::{Path as AxumPath, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures::{SinkExt, StreamExt};
use jailgun_core::{
    AgentError, DeployQueueState, EventKind, JailgunAgentRunRequest, JailgunAgentRunSummary,
    JailgunConfig, JailgunEvent, RunSnapshot, TabSnapshot,
};
use jailgun_orchestrator::{
    execute_prepared_agent_run, prepare_agent_run, AgentRunBackend, AgentRunEventSink,
    AgentRunPaths, DefaultAgentRunBackend, PreparedAgentRun,
};
use serde_json::{json, Value};
use tokio::{
    net::TcpListener,
    sync::{broadcast, RwLock},
};
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct AppState {
    pub config: JailgunConfig,
    pub runs: Arc<RwLock<Vec<RunSnapshot>>>,
    pub agent_summaries: Arc<RwLock<HashMap<String, JailgunAgentRunSummary>>>,
    pub events: Arc<RwLock<Vec<JailgunEvent>>>,
    pub receipt_dir: PathBuf,
    /// When `Some`, the WS endpoint subscribes to it and streams live events.
    /// When `None`, the WS endpoint replays `events` once and closes
    /// (fixture mode used by `jailgun fixture`).
    pub event_bus: Option<broadcast::Sender<JailgunEvent>>,
    /// When `Some`, POST `/api/events` and POST `/api/runs` require
    /// `x-jailgun-token: <token>`.
    /// When `None`, the endpoints refuse every request with 503.
    pub ingest_token: Option<String>,
    pub agent_backend: Arc<dyn AgentRunBackend>,
}

impl AppState {
    pub fn fixture(config: JailgunConfig) -> Self {
        let run = RunSnapshot::fixture();
        Self {
            config,
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
        let state = Self {
            config,
            runs: Arc::new(RwLock::new(Vec::new())),
            agent_summaries: Arc::new(RwLock::new(HashMap::new())),
            events: Arc::new(RwLock::new(Vec::new())),
            receipt_dir,
            event_bus: Some(tx),
            ingest_token: None,
            agent_backend: Arc::new(DefaultAgentRunBackend),
        };
        (state, rx)
    }

    pub fn with_ingest_token(mut self, token: Option<String>) -> Self {
        self.ingest_token = token;
        self
    }

    pub fn with_agent_backend(mut self, backend: Arc<dyn AgentRunBackend>) -> Self {
        self.agent_backend = backend;
        self
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JailgunAgentRunAcceptedResponse {
    pub run_id: String,
    pub status: String,
    pub summary_json: String,
    pub events_jsonl: String,
    pub run_url: String,
    pub summary_url: String,
}

pub fn api_router(state: AppState) -> Router {
    Router::new()
        .route("/api/runs", get(get_runs).post(post(start_agent_run)))
        .route("/api/runs/{run_id}", get(get_run))
        .route("/api/runs/{run_id}/agent-summary", get(get_agent_summary))
        .route("/api/config/effective", get(get_effective_config))
        .route("/api/receipts/{run_id}", get(get_receipts))
        .route("/api/events", post(post_event))
        .route("/ws/events", get(ws_events))
        .with_state(Arc::new(state))
}

pub fn router_with_static(state: AppState, static_dir: impl AsRef<Path>) -> Router {
    api_router(state).fallback_service(ServeDir::new(static_dir.as_ref()))
}

pub async fn serve(addr: SocketAddr, router: Router) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, router).await
}

async fn get_runs(State(state): State<Arc<AppState>>) -> Json<Vec<RunSnapshot>> {
    Json(state.runs.read().await.clone())
}

async fn get_run(
    State(state): State<Arc<AppState>>,
    AxumPath(run_id): AxumPath<String>,
) -> Response {
    let runs = state.runs.read().await;
    match runs.iter().find(|run| run.run_id == run_id) {
        Some(run) => Json(run).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "run not found" })),
        )
            .into_response(),
    }
}

async fn get_agent_summary(
    State(state): State<Arc<AppState>>,
    AxumPath(run_id): AxumPath<String>,
) -> Response {
    if let Some(summary) = state.agent_summaries.read().await.get(&run_id).cloned() {
        return Json(summary).into_response();
    }

    let summary_path = agent_summary_path(&state.receipt_dir.join(&run_id), &run_id);
    match tokio::fs::read_to_string(&summary_path).await {
        Ok(text) => match serde_json::from_str::<JailgunAgentRunSummary>(&text) {
            Ok(summary) => {
                state
                    .agent_summaries
                    .write()
                    .await
                    .insert(run_id.clone(), summary.clone());
                Json(summary).into_response()
            }
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "summary-json-invalid", "run_id": run_id })),
            )
                .into_response(),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let runs = state.runs.read().await;
            if runs.iter().any(|run| run.run_id == run_id) {
                (
                    StatusCode::ACCEPTED,
                    Json(json!({ "run_id": run_id, "status": "running" })),
                )
                    .into_response()
            } else {
                (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": "run not found" })),
                )
                    .into_response()
            }
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "summary-json-read-failed", "run_id": run_id })),
        )
            .into_response(),
    }
}

async fn get_effective_config(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(state.config.redacted_for_display())
}

async fn get_receipts(
    State(state): State<Arc<AppState>>,
    AxumPath(run_id): AxumPath<String>,
) -> Response {
    let dir = state.receipt_dir.join(&run_id);
    let mut receipts = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir(&dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            match tokio::fs::read_to_string(&path).await {
                Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(value) => receipts.push(value),
                    Err(_) => receipts.push(json!({ "path": path, "error": "invalid-json" })),
                },
                Err(_) => receipts.push(json!({ "path": path, "error": "read-failed" })),
            }
        }
    }
    Json(json!({ "run_id": run_id, "receipts": receipts })).into_response()
}

async fn ws_events(State(state): State<Arc<AppState>>, ws: WebSocketUpgrade) -> Response {
    let replay = state.events.read().await.clone();
    let receiver = state.event_bus.as_ref().map(|tx| tx.subscribe());
    ws.on_upgrade(move |socket| handle_ws(socket, replay, receiver))
        .into_response()
}

async fn handle_ws(
    socket: axum::extract::ws::WebSocket,
    replay: Vec<JailgunEvent>,
    receiver: Option<broadcast::Receiver<JailgunEvent>>,
) {
    let (mut sender, mut incoming) = socket.split();
    for event in replay {
        if !send_event(&mut sender, &event).await {
            return;
        }
    }
    let Some(mut rx) = receiver else {
        return;
    };
    loop {
        tokio::select! {
            recv = rx.recv() => {
                match recv {
                    Ok(event) => {
                        if !send_event(&mut sender, &event).await {
                            return;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(dropped)) => {
                        let warn = JailgunEvent::new(
                            "system".to_string(),
                            EventKind::Error,
                            "websocket lagged".to_string(),
                        )
                        .with_field("dropped", dropped.to_string());
                        if !send_event(&mut sender, &warn).await {
                            return;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
            msg = incoming.next() => {
                match msg {
                    Some(Ok(axum::extract::ws::Message::Close(_))) | None => return,
                    Some(Err(_)) => return,
                    Some(Ok(_)) => continue,
                }
            }
        }
    }
}

async fn send_event(
    sender: &mut futures::stream::SplitSink<axum::extract::ws::WebSocket, Message>,
    event: &JailgunEvent,
) -> bool {
    match serde_json::to_string(event) {
        Ok(text) => sender.send(Message::Text(text.into())).await.is_ok(),
        Err(_) => true,
    }
}

async fn post_event(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(event): Json<JailgunEvent>,
) -> StatusCode {
    let Some(expected) = state.ingest_token.as_deref() else {
        return StatusCode::SERVICE_UNAVAILABLE;
    };
    let provided = headers
        .get("x-jailgun-token")
        .and_then(|value| value.to_str().ok());
    if provided != Some(expected) {
        return StatusCode::UNAUTHORIZED;
    }
    record_event(&state, event.clone()).await;
    if let Some(tx) = state.event_bus.as_ref() {
        let _ = tx.send(event);
        StatusCode::ACCEPTED
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

async fn start_agent_run(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if state.event_bus.is_none() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(agent_error(
                "agent-run-unavailable",
                "start a Jailgun agent run",
                "live mode is required to start server-side runs",
            )),
        )
            .into_response();
    }
    let Some(expected) = state.ingest_token.as_deref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(agent_error(
                "agent-run-unavailable",
                "start a Jailgun agent run",
                "x-jailgun-token is required when run ingestion is enabled",
            )),
        )
            .into_response();
    };
    let provided = headers
        .get("x-jailgun-token")
        .and_then(|value| value.to_str().ok());
    if provided != Some(expected) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(agent_error(
                "agent-run-unauthorized",
                "start a Jailgun agent run",
                "x-jailgun-token did not match the configured token",
            )),
        )
            .into_response();
    }

    let mut request = match parse_agent_run_request(body) {
        Ok(request) => request,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(agent_error(
                    "agent-run-invalid",
                    "start a Jailgun agent run",
                    error,
                )),
            )
                .into_response();
        }
    };

    let run_id = request
        .run_id
        .clone()
        .unwrap_or_else(|| format!("run-{}", uuid::Uuid::new_v4()));
    request.run_id = Some(run_id.clone());
    let run_dir = state.receipt_dir.join(&run_id);
    let output_paths = AgentRunPaths {
        events_jsonl: agent_events_path(&run_dir),
        summary_json: agent_summary_path(&run_dir, &run_id),
    };
    let accepted_paths = output_paths.clone();
    let prepared = match prepare_agent_run(request, output_paths) {
        Ok(prepared) => prepared,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(agent_error(
                    "agent-run-invalid",
                    "start a Jailgun agent run",
                    error.to_string(),
                )),
            )
                .into_response();
        }
    };

    insert_run_snapshot(&state, prepared_snapshot(&prepared)).await;

    let backend = state.agent_backend.clone();
    let sink = ServerAgentEventSink {
        state: state.clone(),
    };
    let failure_run_id = run_id.clone();
    tokio::spawn(async move {
        if let Err(error) = execute_prepared_agent_run(prepared, backend.as_ref(), &sink).await {
            mark_agent_run_failed(&sink.state, &failure_run_id, error.to_string()).await;
        }
    });

    let response_run_id = run_id.clone();
    let response = JailgunAgentRunAcceptedResponse {
        run_id,
        status: "accepted".into(),
        summary_json: accepted_paths.summary_json.display().to_string(),
        events_jsonl: accepted_paths.events_jsonl.display().to_string(),
        run_url: format!("/api/runs/{response_run_id}"),
        summary_url: format!("/api/runs/{response_run_id}/agent-summary"),
    };
    (StatusCode::ACCEPTED, Json(response)).into_response()
}

fn parse_agent_run_request(body: Value) -> Result<JailgunAgentRunRequest, String> {
    match body.get("version").and_then(Value::as_u64) {
        Some(version) if version == jailgun_core::JAILGUN_AGENT_INTERFACE_VERSION as u64 => {}
        Some(version) => {
            return Err(format!(
                "unsupported Jailgun agent interface version {}; expected {}",
                version,
                jailgun_core::JAILGUN_AGENT_INTERFACE_VERSION
            ));
        }
        None => {
            return Err(format!(
                "Jailgun agent request requires version: {}",
                jailgun_core::JAILGUN_AGENT_INTERFACE_VERSION
            ));
        }
    }
    serde_json::from_value(body).map_err(|error| format!("parsing agent request JSON: {error}"))
}

#[derive(Clone)]
struct ServerAgentEventSink {
    state: Arc<AppState>,
}

#[async_trait::async_trait]
impl AgentRunEventSink for ServerAgentEventSink {
    async fn on_event(&self, event: &JailgunEvent) -> anyhow::Result<()> {
        record_event(&self.state, event.clone()).await;
        if let Some(tx) = self.state.event_bus.as_ref() {
            let _ = tx.send(event.clone());
        }
        Ok(())
    }

    async fn on_summary(&self, summary: &JailgunAgentRunSummary) -> anyhow::Result<()> {
        self.state
            .agent_summaries
            .write()
            .await
            .insert(summary.run_id.clone(), summary.clone());
        let mut runs = self.state.runs.write().await;
        if let Some(run) = runs.iter_mut().find(|run| run.run_id == summary.run_id) {
            run.finished_at = Some(summary.finished_at.clone());
            run.status = summary.status.clone();
            run.denied_github_prompts = summary.denied_github_prompts;
            run.allowed_info_prompts = summary.allowed_info_prompts;
        }
        Ok(())
    }
}

fn prepared_snapshot(prepared: &PreparedAgentRun) -> RunSnapshot {
    RunSnapshot {
        run_id: prepared.opts.run_id.clone(),
        started_at: prepared.started_at.clone(),
        finished_at: None,
        status: "running".into(),
        tabs: Vec::new(),
        deploy_queue: DeployQueueState::Running,
        denied_github_prompts: 0,
        allowed_info_prompts: 0,
    }
}

async fn insert_run_snapshot(state: &Arc<AppState>, snapshot: RunSnapshot) {
    let mut runs = state.runs.write().await;
    if let Some(existing) = runs.iter_mut().find(|run| run.run_id == snapshot.run_id) {
        *existing = snapshot;
    } else {
        runs.insert(0, snapshot);
    }
}

async fn mark_agent_run_failed(state: &Arc<AppState>, run_id: &str, reason: String) {
    let event = JailgunEvent::new(run_id.to_string(), EventKind::Error, reason.clone())
        .with_severity(jailgun_core::Severity::Error);
    record_event(state, event.clone()).await;
    if let Some(tx) = state.event_bus.as_ref() {
        let _ = tx.send(event.clone());
    }

    let mut runs = state.runs.write().await;
    if let Some(run) = runs.iter_mut().find(|run| run.run_id == run_id) {
        run.finished_at = Some(event.timestamp.clone());
        run.status = "failed".into();
    }
}

async fn record_event(state: &Arc<AppState>, event: JailgunEvent) {
    state.events.write().await.push(event.clone());
    let mut runs = state.runs.write().await;
    if let Some(run) = runs.iter_mut().find(|run| run.run_id == event.run_id) {
        apply_event_to_run(run, &event);
    } else {
        let mut run = RunSnapshot {
            run_id: event.run_id.clone(),
            started_at: event.timestamp.clone(),
            finished_at: None,
            status: "running".into(),
            tabs: Vec::new(),
            deploy_queue: DeployQueueState::Idle,
            denied_github_prompts: 0,
            allowed_info_prompts: 0,
        };
        apply_event_to_run(&mut run, &event);
        runs.insert(0, run);
    }
}

fn apply_event_to_run(run: &mut RunSnapshot, event: &JailgunEvent) {
    match event.kind {
        EventKind::RunStarted => {
            run.started_at = event.timestamp.clone();
            run.status = event
                .fields
                .get("status")
                .cloned()
                .unwrap_or_else(|| "running".into());
        }
        EventKind::DeployQueued => {
            run.deploy_queue = DeployQueueState::Waiting;
        }
        EventKind::DeployFinished => {
            run.deploy_queue = DeployQueueState::Done;
            run.finished_at = Some(event.timestamp.clone());
        }
        EventKind::Error => {
            run.status = "failed".into();
            run.finished_at = Some(event.timestamp.clone());
        }
        EventKind::PromptPolicy => {
            bump_policy_counts(run, event);
        }
        _ => {}
    }

    if let Some(tab_id) = event.tab_id {
        let tab = upsert_tab(run, tab_id);
        apply_tab_event(tab, event);
    }
}

fn upsert_tab(run: &mut RunSnapshot, tab_id: u16) -> &mut TabSnapshot {
    if let Some(index) = run.tabs.iter().position(|tab| tab.tab_id == tab_id) {
        return &mut run.tabs[index];
    }
    run.tabs.push(TabSnapshot {
        tab_id,
        status: "active".into(),
        page_url: String::new(),
        archive_sha256: None,
        download_latency_ms: None,
        deploy_status: "pending".into(),
        prompt_policy_decision: None,
    });
    let len = run.tabs.len();
    &mut run.tabs[len - 1]
}

fn apply_tab_event(tab: &mut TabSnapshot, event: &JailgunEvent) {
    if let Some(status) = event.fields.get("tab_status") {
        tab.status = status.clone();
    }
    if let Some(page_url) = event.fields.get("page_url") {
        tab.page_url = page_url.clone();
    }
    if let Some(sha256) = event.fields.get("sha256") {
        tab.archive_sha256 = Some(sha256.clone());
    }
    if let Some(download_latency_ms) = event.fields.get("download_latency_ms") {
        tab.download_latency_ms = download_latency_ms.parse::<u64>().ok();
    }
    if let Some(deploy_status) = event.fields.get("deploy_status") {
        tab.deploy_status = deploy_status.clone();
    }
    if let Some(decision) = event.fields.get("decision") {
        tab.prompt_policy_decision = Some(decision.clone());
    }

    match event.kind {
        EventKind::TabOpened if tab.status == "active" => {
            tab.status = "opening".into();
        }
        EventKind::TabOpened => {}
        EventKind::PromptSubmitted => {
            tab.status = "submitted".into();
        }
        EventKind::TarDiscovered => {
            tab.status = "tar-discovered".into();
        }
        EventKind::DownloadReceipt => {
            tab.status = "downloaded".into();
        }
        EventKind::DeployFinished => {
            tab.deploy_status = event
                .fields
                .get("outcome")
                .cloned()
                .unwrap_or_else(|| "succeeded".into());
        }
        EventKind::RemoteSafety => {
            if let Some(policy) = event.fields.get("policy") {
                tab.deploy_status = policy.clone();
            }
        }
        EventKind::Error => {
            tab.status = "error".into();
        }
        _ => {}
    }
}

fn bump_policy_counts(run: &mut RunSnapshot, event: &JailgunEvent) {
    match event.fields.get("decision").map(String::as_str) {
        Some("deny") => run.denied_github_prompts += 1,
        Some("allow-info") => run.allowed_info_prompts += 1,
        _ => {}
    }
}

fn agent_events_path(run_dir: &Path) -> PathBuf {
    run_dir.join("agent-events.jsonl")
}

fn agent_summary_path(run_dir: &Path, _run_id: &str) -> PathBuf {
    run_dir.join("agent-summary.json")
}

fn agent_error(code: &'static str, purpose: &'static str, reason: impl Into<String>) -> AgentError {
    AgentError::new(
        code,
        purpose,
        reason.into(),
        vec![
            "verify the live server token and request body",
            "check the configured prompt file path",
            "run the mapped rust test lane if the failure persists",
        ],
        "docs/testing.md",
        "rerun the agent request with a valid prompt file and token",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    struct FakeBackend;

    #[async_trait::async_trait]
    impl jailgun_orchestrator::AgentRunBackend for FakeBackend {
        async fn start(
            &self,
            opts: jailgun_orchestrator::config::RunOptions,
        ) -> anyhow::Result<jailgun_orchestrator::OrchestratorHandle> {
            let (events_tx, events_rx) = broadcast::channel(8);
            let (completion_tx, completion_rx) = tokio::sync::oneshot::channel();
            let (shutdown_tx, _shutdown_rx) = tokio::sync::watch::channel(false);
            let run_id = opts.run_id.clone();
            tokio::spawn(async move {
                let event =
                    JailgunEvent::new(run_id.clone(), EventKind::RunStarted, "fake started");
                let _ = events_tx.send(event);
                let summary = jailgun_orchestrator::RunSummary {
                    run_id,
                    total_tabs: 1,
                    downloaded: 0,
                    deployed: 0,
                    failures: Vec::new(),
                    denied_github_prompts: 0,
                    allowed_info_prompts: 0,
                };
                let _ = completion_tx.send(summary);
            });
            Ok(jailgun_orchestrator::OrchestratorHandle {
                events_rx,
                completion: completion_rx,
                shutdown: shutdown_tx,
            })
        }
    }

    #[tokio::test]
    async fn serves_run_snapshot_and_redacted_config() {
        let app = api_router(AppState::fixture(JailgunConfig::default()));
        let runs_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/runs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(runs_response.status(), StatusCode::OK);

        let config_response = app
            .oneshot(
                Request::builder()
                    .uri("/api/config/effective")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(config_response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn ingest_requires_matching_token() {
        let (state, _rx) = AppState::live(JailgunConfig::default(), PathBuf::from("receipts"), 64);
        let history = state.events.clone();
        let app = api_router(state.with_ingest_token(Some("secret".to_string())));

        let event = JailgunEvent::new("run-1", EventKind::RunStarted, "hi");
        let body = serde_json::to_vec(&event).unwrap();

        let bad = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/events")
                    .header("content-type", "application/json")
                    .header("x-jailgun-token", "wrong")
                    .body(Body::from(body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);

        let ok = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/events")
                    .header("content-type", "application/json")
                    .header("x-jailgun-token", "secret")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ok.status(), StatusCode::ACCEPTED);
        let events = history.read().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].run_id, "run-1");
    }

    #[tokio::test]
    async fn ingest_without_token_returns_503() {
        let (state, _rx) = AppState::live(JailgunConfig::default(), PathBuf::from("receipts"), 64);
        let app = api_router(state);
        let event = JailgunEvent::new("run-1", EventKind::RunStarted, "hi");
        let body = serde_json::to_vec(&event).unwrap();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/events")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn live_bus_forwards_events_to_receivers() {
        let (state, mut rx) =
            AppState::live(JailgunConfig::default(), PathBuf::from("receipts"), 64);
        let tx = state.event_bus.clone().expect("live bus");
        let event = JailgunEvent::new("run-A", EventKind::DeployFinished, "ok");
        tx.send(event.clone()).expect("send ok");
        let received = rx.recv().await.expect("recv ok");
        assert_eq!(received, event);
    }

    #[tokio::test]
    async fn start_run_accepts_request_and_publishes_snapshot() {
        let backend = Arc::new(FakeBackend);
        let (state, _rx) = AppState::live(JailgunConfig::default(), PathBuf::from("receipts"), 64);
        let state = state
            .with_ingest_token(Some("secret".into()))
            .with_agent_backend(backend.clone());
        let app = api_router(state);
        let prompt_file =
            std::env::temp_dir().join(format!("jailgun-prompt-{}.txt", uuid::Uuid::new_v4()));
        std::fs::write(&prompt_file, "review this change").unwrap();

        let mut request = JailgunAgentRunRequest {
            version: jailgun_core::JAILGUN_AGENT_INTERFACE_VERSION,
            run_id: Some("run-1".into()),
            prompt_ref: "jmcp://prompt/1".into(),
            prompt_file,
            config_path: None,
            tabs: Some(1),
            max_runtime_seconds: Some(60),
            repo: Default::default(),
            source_archive: Default::default(),
            deploy: Default::default(),
            ci: Default::default(),
            browser: Default::default(),
            github: Default::default(),
        };
        request.browser.bridge_cmd = vec!["fake-bridge".into()];

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/runs")
                    .header("content-type", "application/json")
                    .header("x-jailgun-token", "secret")
                    .body(Body::from(serde_json::to_vec(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::ACCEPTED,
            "agent run request was rejected: {}",
            String::from_utf8_lossy(&body)
        );

        let accepted: JailgunAgentRunAcceptedResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(accepted.run_id, "run-1");

        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .uri("/api/runs/run-1/agent-summary")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap()
                    .status()
                    == StatusCode::OK
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("summary became available");
    }

    #[tokio::test]
    async fn start_run_requires_v1_request_version() {
        let (state, _rx) = AppState::live(JailgunConfig::default(), PathBuf::from("receipts"), 64);
        let app = api_router(state.with_ingest_token(Some("secret".into())));
        let prompt_file =
            std::env::temp_dir().join(format!("jailgun-prompt-{}.txt", uuid::Uuid::new_v4()));
        std::fs::write(&prompt_file, "review this change").unwrap();

        let missing_version = json!({
            "prompt_ref": "jmcp://prompt/1",
            "prompt_file": prompt_file
        });
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/runs")
                    .header("content-type", "application/json")
                    .header("x-jailgun-token", "secret")
                    .body(Body::from(serde_json::to_vec(&missing_version).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let bad_version = json!({
            "version": 2,
            "prompt_ref": "jmcp://prompt/1",
            "prompt_file": prompt_file
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/runs")
                    .header("content-type", "application/json")
                    .header("x-jailgun-token", "secret")
                    .body(Body::from(serde_json::to_vec(&bad_version).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

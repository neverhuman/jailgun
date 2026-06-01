pub mod bus;

pub use bus::{BroadcastBus, EventBus, NoopBus, RecordingBus};

use std::{
    io,
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
    DeployQueueState, EventKind, JailgunConfig, JailgunEvent, RunSnapshot, Severity, TabSnapshot,
};
use serde_json::json;
use tokio::{
    net::TcpListener,
    sync::{broadcast, RwLock},
};
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct AppState {
    pub config: JailgunConfig,
    pub runs: Arc<RwLock<Vec<RunSnapshot>>>,
    pub events: Arc<RwLock<Vec<JailgunEvent>>>,
    pub receipt_dir: PathBuf,
    /// When `Some`, the WS endpoint subscribes to it and streams live events.
    /// When `None`, the WS endpoint replays `events` once and closes
    /// (fixture mode used by `jailgun fixture`).
    pub event_bus: Option<broadcast::Sender<JailgunEvent>>,
    /// When `Some`, POST `/api/events` requires `x-jailgun-token: <token>`.
    /// When `None`, the endpoint refuses every request with 503.
    pub ingest_token: Option<String>,
}

impl AppState {
    pub fn fixture(config: JailgunConfig) -> Self {
        let run = RunSnapshot::fixture();
        Self {
            config,
            events: Arc::new(RwLock::new(vec![
                JailgunEvent::new(&run.run_id, EventKind::RunStarted, "fixture run started")
                    .with_field("tabs", run.planned_tabs.to_string())
                    .with_field("batch_tabs", run.batch_tabs.to_string())
                    .with_field("loop_count", run.loop_count.to_string())
                    .with_field("planned_tabs", run.planned_tabs.to_string()),
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
            runs: Arc::new(RwLock::new(vec![run])),
            receipt_dir: PathBuf::from("receipts"),
            event_bus: None,
            ingest_token: None,
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
            events: Arc::new(RwLock::new(Vec::new())),
            receipt_dir,
            event_bus: Some(tx),
            ingest_token: None,
        };
        (state, rx)
    }

    pub fn with_ingest_token(mut self, token: Option<String>) -> Self {
        self.ingest_token = token;
        self
    }

    pub async fn record_event(&self, event: JailgunEvent) {
        self.events.write().await.push(event.clone());
        apply_event_to_runs(&self.runs, &event).await;
        if let Some(tx) = self.event_bus.as_ref() {
            let _ = tx.send(event);
        }
    }
}

pub fn api_router(state: AppState) -> Router {
    Router::new()
        .route("/api/runs", get(get_runs))
        .route("/api/runs/{run_id}", get(get_run))
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
    let (_addr, task) = spawn_server(addr, router).await?;
    match task.await {
        Ok(result) => result,
        Err(error) => Err(io::Error::other(format!("server task failed: {error}"))),
    }
}

pub async fn spawn_server(
    addr: SocketAddr,
    router: Router,
) -> std::io::Result<(SocketAddr, tokio::task::JoinHandle<std::io::Result<()>>)> {
    let listener = TcpListener::bind(addr).await?;
    let bound_addr = listener.local_addr()?;
    let task = tokio::spawn(async move { axum::serve(listener, router).await });
    Ok((bound_addr, task))
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
    if state.event_bus.is_some() {
        state.record_event(event).await;
        StatusCode::ACCEPTED
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

async fn apply_event_to_runs(runs: &Arc<RwLock<Vec<RunSnapshot>>>, event: &JailgunEvent) {
    let mut runs = runs.write().await;
    if let Some(index) = runs.iter().position(|run| run.run_id == event.run_id) {
        let run = runs.remove(index);
        runs.insert(index, apply_event_to_run(run, event));
    } else {
        runs.insert(0, create_run_from_event(event));
    }
}

fn create_run_from_event(event: &JailgunEvent) -> RunSnapshot {
    let metadata = run_loop_metadata(event, None);
    let mut run = RunSnapshot {
        run_id: event.run_id.clone(),
        started_at: event.timestamp.clone(),
        finished_at: None,
        status: event
            .fields
            .get("status")
            .cloned()
            .unwrap_or_else(|| "running".to_string()),
        batch_tabs: metadata.batch_tabs,
        loop_count: metadata.loop_count,
        planned_tabs: metadata.planned_tabs,
        deploy_queue: queue_state_for_event(event, DeployQueueState::Idle),
        denied_github_prompts: if prompt_policy_decision(event) == Some("deny") {
            1
        } else {
            0
        },
        allowed_info_prompts: if matches!(
            prompt_policy_decision(event),
            Some("allow-info" | "allowed-info" | "allow")
        ) {
            1
        } else {
            0
        },
        tabs: event
            .tab_id
            .map(|tab_id| vec![apply_event_to_tab(default_tab_snapshot(tab_id), event)])
            .unwrap_or_default(),
        early_stops_succeeded: 0,
        early_stops_attempted: 0,
    };
    recompute_early_stop_counts(&mut run);
    run
}

fn apply_event_to_run(mut run: RunSnapshot, event: &JailgunEvent) -> RunSnapshot {
    if matches!(&event.kind, EventKind::RunStarted) {
        let metadata = run_loop_metadata(event, Some(&run));
        run.batch_tabs = metadata.batch_tabs;
        run.loop_count = metadata.loop_count;
        run.planned_tabs = metadata.planned_tabs;
    }
    run.status = event.fields.get("status").cloned().unwrap_or(run.status);
    if matches!(&event.kind, EventKind::DeployFinished)
        && !matches!(&event.severity, Severity::Error)
    {
        run.finished_at = Some(event.timestamp.clone());
    }
    run.deploy_queue = queue_state_for_event(event, run.deploy_queue);
    match prompt_policy_decision(event) {
        Some("deny" | "denied") => {
            run.denied_github_prompts = run.denied_github_prompts.saturating_add(1);
        }
        Some("allow-info" | "allowed-info" | "allow") => {
            run.allowed_info_prompts = run.allowed_info_prompts.saturating_add(1);
        }
        _ => {}
    }
    if let Some(tab_id) = event.tab_id {
        upsert_tab(&mut run.tabs, tab_id, event);
    }
    recompute_early_stop_counts(&mut run);
    run
}

fn recompute_early_stop_counts(run: &mut RunSnapshot) {
    let mut succeeded: u16 = 0;
    let mut attempted: u16 = 0;
    for tab in &run.tabs {
        match tab.early_stop_outcome.as_deref() {
            Some("succeeded") => {
                succeeded = succeeded.saturating_add(1);
                attempted = attempted.saturating_add(1);
            }
            Some("attempted") => {
                attempted = attempted.saturating_add(1);
            }
            _ => {}
        }
    }
    run.early_stops_succeeded = succeeded;
    run.early_stops_attempted = attempted;
}

#[derive(Debug, Clone, Copy)]
struct RunLoopMetadata {
    batch_tabs: u16,
    loop_count: u16,
    planned_tabs: u16,
}

fn run_loop_metadata(event: &JailgunEvent, existing: Option<&RunSnapshot>) -> RunLoopMetadata {
    let batch_tabs = event
        .fields
        .get("batch_tabs")
        .and_then(|value| value.parse::<u16>().ok())
        .or_else(|| {
            event
                .fields
                .get("tabs")
                .and_then(|value| value.parse::<u16>().ok())
        })
        .or_else(|| existing.map(|run| run.batch_tabs))
        .unwrap_or(0);
    let loop_count = event
        .fields
        .get("loop_count")
        .and_then(|value| value.parse::<u16>().ok())
        .or_else(|| existing.map(|run| run.loop_count))
        .unwrap_or(0);
    let computed_planned_tabs = if batch_tabs > 0 {
        batch_tabs.checked_mul(loop_count.saturating_add(1))
    } else {
        None
    };
    let planned_tabs = event
        .fields
        .get("planned_tabs")
        .and_then(|value| value.parse::<u16>().ok())
        .or(computed_planned_tabs)
        .or_else(|| existing.map(|run| run.planned_tabs))
        .unwrap_or(0);
    RunLoopMetadata {
        batch_tabs,
        loop_count,
        planned_tabs,
    }
}

fn upsert_tab(tabs: &mut Vec<TabSnapshot>, tab_id: u16, event: &JailgunEvent) {
    if let Some(tab) = tabs.iter_mut().find(|tab| tab.tab_id == tab_id) {
        *tab = apply_event_to_tab(tab.clone(), event);
    } else {
        tabs.push(apply_event_to_tab(default_tab_snapshot(tab_id), event));
        tabs.sort_by_key(|tab| tab.tab_id);
    }
}

fn default_tab_snapshot(tab_id: u16) -> TabSnapshot {
    TabSnapshot {
        tab_id,
        status: "active".to_string(),
        page_url: String::new(),
        archive_sha256: None,
        download_latency_ms: None,
        deploy_status: "pending".to_string(),
        prompt_policy_decision: None,
        early_stop_outcome: None,
    }
}

fn apply_event_to_tab(mut tab: TabSnapshot, event: &JailgunEvent) -> TabSnapshot {
    tab.status = tab_status_for_event(event, tab.status);
    if let Some(page_url) = event.fields.get("page_url") {
        tab.page_url = page_url.clone();
    }
    if let Some(sha) = event
        .fields
        .get("sha256")
        .or_else(|| event.fields.get("local_sha256"))
    {
        tab.archive_sha256 = Some(sha.clone());
    }
    if let Some(value) = event.fields.get("download_latency_ms") {
        if let Ok(parsed) = value.parse::<u64>() {
            tab.download_latency_ms = Some(parsed);
        }
    }
    tab.deploy_status = deploy_status_for_event(event, tab.deploy_status);
    if let Some(decision) = event.fields.get("decision") {
        tab.prompt_policy_decision = Some(decision.clone());
    }
    if matches!(event.kind, EventKind::GenerationStopped) {
        if let Some(outcome) = early_stop_outcome_for_event(event) {
            tab.early_stop_outcome = Some(merge_early_stop_outcome(
                tab.early_stop_outcome.as_deref(),
                outcome,
            ));
        }
    }
    tab
}

fn early_stop_outcome_for_event(event: &JailgunEvent) -> Option<&'static str> {
    let phase = event.fields.get("phase").map(String::as_str).unwrap_or("");
    if !matches!(phase, "pre-download" | "post-download") {
        return None;
    }
    let method = event.fields.get("method").map(String::as_str).unwrap_or("");
    if early_stop_method_is_success(method) {
        Some("succeeded")
    } else {
        Some("attempted")
    }
}

fn merge_early_stop_outcome(current: Option<&str>, incoming: &'static str) -> String {
    match (current, incoming) {
        (Some("succeeded"), _) => "succeeded".to_string(),
        (_, "succeeded") => "succeeded".to_string(),
        _ => "attempted".to_string(),
    }
}

fn early_stop_method_is_success(method: &str) -> bool {
    !method.is_empty()
        && !method.starts_with("not-active")
        && !method.starts_with("not-run")
        && !method.starts_with("shutdown")
}

fn tab_status_for_event(event: &JailgunEvent, current: String) -> String {
    if let Some(status) = event.fields.get("tab_status") {
        return status.clone();
    }
    match &event.kind {
        EventKind::TabOpened => "opened".to_string(),
        EventKind::PromptSubmitted => "prompt-submitted".to_string(),
        EventKind::TarDiscovered => "tar-discovered".to_string(),
        EventKind::DownloadStarted => "downloading".to_string(),
        EventKind::DownloadReceipt => "downloaded".to_string(),
        EventKind::GenerationStopped => "generation-stopped".to_string(),
        EventKind::TabClosed => "closed".to_string(),
        EventKind::DeployFinished if matches!(&event.severity, Severity::Error) => {
            "error".to_string()
        }
        EventKind::DeployFinished => "deployed".to_string(),
        _ => current,
    }
}

fn deploy_status_for_event(event: &JailgunEvent, current: String) -> String {
    if let Some(status) = event.fields.get("deploy_status") {
        return status.clone();
    }
    match &event.kind {
        EventKind::DeployQueued => event
            .fields
            .get("status")
            .cloned()
            .unwrap_or_else(|| "queued".to_string()),
        EventKind::RemoteSafety => event.fields.get("phase").cloned().unwrap_or(current),
        EventKind::DeployFinished => event
            .fields
            .get("outcome")
            .cloned()
            .unwrap_or_else(|| "done".to_string()),
        _ => current,
    }
}

fn queue_state_for_event(event: &JailgunEvent, current: DeployQueueState) -> DeployQueueState {
    match &event.kind {
        EventKind::DeployQueued => DeployQueueState::Waiting,
        EventKind::RemoteSafety
            if event.fields.get("outcome").map(String::as_str) == Some("blocked") =>
        {
            DeployQueueState::Blocked
        }
        EventKind::RemoteSafety => DeployQueueState::Running,
        EventKind::DeployFinished => DeployQueueState::Done,
        _ => current,
    }
}

fn prompt_policy_decision(event: &JailgunEvent) -> Option<&str> {
    event.fields.get("decision").map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

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
    async fn run_started_loop_metadata_is_seeded_and_preserved() {
        let (state, _rx) = AppState::live(JailgunConfig::default(), PathBuf::from("receipts"), 64);
        let run_started = JailgunEvent::new("run-1", EventKind::RunStarted, "hi")
            .with_field("tabs", "21")
            .with_field("batch_tabs", "7")
            .with_field("loop_count", "2")
            .with_field("planned_tabs", "21");
        state.record_event(run_started).await;

        let later = JailgunEvent::new("run-1", EventKind::DownloadReceipt, "download complete")
            .with_tab(3)
            .with_field("sha256", "abc123");
        state.record_event(later).await;

        let runs = state.runs.read().await;
        let run = runs
            .iter()
            .find(|run| run.run_id == "run-1")
            .expect("run snapshot");
        assert_eq!(run.batch_tabs, 7);
        assert_eq!(run.loop_count, 2);
        assert_eq!(run.planned_tabs, 21);
        assert_eq!(run.tabs.len(), 1);
        assert_eq!(run.tabs[0].tab_id, 3);
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
}

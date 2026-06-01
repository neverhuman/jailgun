pub mod bus;

pub use bus::{BroadcastBus, EventBus, NoopBus, RecordingBus};

use std::{
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
use jailgun_core::{EventKind, JailgunConfig, JailgunEvent, RunSnapshot};
use serde_json::json;
use tokio::{
    net::TcpListener,
    sync::{broadcast, RwLock},
};
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct AppState {
    pub config: JailgunConfig,
    pub runs: Vec<RunSnapshot>,
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
            runs: vec![run],
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
            runs: Vec::new(),
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
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, router).await
}

async fn get_runs(State(state): State<Arc<AppState>>) -> Json<Vec<RunSnapshot>> {
    Json(state.runs.clone())
}

async fn get_run(
    State(state): State<Arc<AppState>>,
    AxumPath(run_id): AxumPath<String>,
) -> Response {
    match state.runs.iter().find(|run| run.run_id == run_id) {
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
    if let Some(tx) = state.event_bus.as_ref() {
        state.events.write().await.push(event.clone());
        let _ = tx.send(event);
        StatusCode::ACCEPTED
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
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

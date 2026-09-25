use std::sync::Arc;

use axum::extract::ws::Message;
use axum::{
    extract::{Query, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use futures::{SinkExt, StreamExt};
use jailgun_core::{EventKind, JailgunEvent};
use serde_json::json;
use tokio::sync::broadcast;

use crate::state::AppState;

#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct WsAuthQuery {
    #[serde(default)]
    pub(crate) token: Option<String>,
    #[serde(default)]
    pub(crate) jailgun_token: Option<String>,
}

pub(crate) async fn ws_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<WsAuthQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    if let Some(response) = websocket_unauthorized(&state, &headers, &query) {
        return response;
    }
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

pub(crate) fn websocket_unauthorized(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    query: &WsAuthQuery,
) -> Option<Response> {
    let Some(expected) = state.ingest_token.as_deref() else {
        return Some(
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": "websocket-token-required" })),
            )
                .into_response(),
        );
    };
    let header_token = headers
        .get("x-jailgun-token")
        .and_then(|value| value.to_str().ok());
    let query_token = query
        .token
        .as_deref()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            query
                .jailgun_token
                .as_deref()
                .filter(|value| !value.is_empty())
        });
    if header_token == Some(expected) || query_token == Some(expected) {
        return None;
    }
    Some(
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "websocket-unauthorized" })),
        )
            .into_response(),
    )
}

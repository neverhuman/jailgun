//! Binary RFB proxy. Credentials and screen contents are never logged or stored.
use crate::{
    auth::{self, Principal},
    AppState,
};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    http::{HeaderMap, StatusCode, Uri},
    response::Response,
    Extension,
};
use jailgun_orchestrator::concept::{AccountSupervisor, LoginView};
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub(crate) async fn open(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    upgrade: Result<WebSocketUpgrade, axum::extract::ws::rejection::WebSocketUpgradeRejection>,
) -> Response {
    if !matches!(
        principal,
        Principal::Operator {
            browser_session: true
        }
    ) {
        return auth::error(
            StatusCode::FORBIDDEN,
            "operator-session-required",
            "Pair the operator dashboard to open the login view.",
        );
    }
    if !headers.contains_key("origin") || !auth::origin_allowed(&headers) {
        return auth::error(
            StatusCode::FORBIDDEN,
            "origin-required",
            "Open the paired dashboard from its loopback address.",
        );
    }
    if uri.query().is_some() {
        return auth::error(
            StatusCode::BAD_REQUEST,
            "login-view-query-rejected",
            "Use the dashboard session; the login view accepts no URL parameters.",
        );
    }
    let Some(supervisor) = state.account_supervisor.clone() else {
        return auth::error(
            StatusCode::SERVICE_UNAVAILABLE,
            "workflow-unavailable",
            "Start the concept daemon with --server-browser on Linux.",
        );
    };
    let Ok(upgrade) = upgrade else {
        return auth::error(
            StatusCode::BAD_REQUEST,
            "websocket-required",
            "Open the login view through the dashboard.",
        );
    };
    let view = match supervisor.open_login_view(&id).await {
        Ok(view) => view,
        Err(_) => return auth::error(StatusCode::CONFLICT, "login-view-unavailable", "Reconnect the account or close its other login view. Run jailgun doctor --server-browser if it cannot start."),
    };
    upgrade
        .max_message_size(64 * 1024)
        .max_frame_size(64 * 1024)
        .on_upgrade(move |socket| proxy(socket, view, supervisor, id, state, headers))
}

async fn proxy(
    mut socket: WebSocket,
    mut view: LoginView,
    supervisor: AccountSupervisor,
    id: String,
    state: Arc<AppState>,
    headers: HeaderMap,
) {
    let mut interval = tokio::time::interval(Duration::from_millis(500));
    let mut buffer = [0_u8; 32 * 1024];
    loop {
        tokio::select! {
            biased;
            _=interval.tick()=> {
                if !state.dashboard_sessions.accepts(&headers) || !supervisor.login_view_active(&id).await { break; }
            }
            message=socket.recv()=> {
                match message {
                    Some(Ok(Message::Binary(bytes))) => {
                        if !matches!(tokio::time::timeout(Duration::from_secs(2), view.stream.write_all(&bytes)).await, Ok(Ok(()))) { break; }
                    }
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => {},
                    _=>break,
                }
            }
            read=view.stream.read(&mut buffer)=> {
                let Ok(length) = read else { break };
                if length == 0 { break; }
                if !matches!(tokio::time::timeout(Duration::from_secs(2), socket.send(Message::Binary(buffer[..length].to_vec().into()))).await, Ok(Ok(()))) { break; }
            }
        }
    }
    view.shutdown().await;
    let _ = tokio::time::timeout(Duration::from_secs(1), socket.send(Message::Close(None))).await;
}

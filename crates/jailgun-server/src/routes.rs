use std::{net::SocketAddr, path::Path, sync::Arc};

use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

use crate::{
    browser::{
        browser_write_unauthorized, get_browser_account, get_browser_accounts,
        post_browser_account_auth_code, post_browser_account_auth_start,
        post_browser_account_restart, post_browser_account_start, post_browser_account_stop,
    },
    mcp::post_mcp,
    runs::{get_agent_summary, get_receipts, get_run, get_runs, post_event, start_agent_run},
    state::AppState,
    ws::ws_events,
};

pub fn api_router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(get_health))
        .route("/api/runs", get(get_runs).post(post(start_agent_run)))
        .route("/api/runs/{run_id}", get(get_run))
        .route("/api/runs/{run_id}/agent-summary", get(get_agent_summary))
        .route("/api/browser/accounts", get(get_browser_accounts))
        .route("/api/browsers", get(get_browser_accounts))
        .route(
            "/api/browser/accounts/{account_id}",
            get(get_browser_account),
        )
        .route("/api/browsers/{account_id}", get(get_browser_account))
        .route(
            "/api/browsers/{account_id}/auth/status",
            get(get_browser_account),
        )
        .route(
            "/api/browser/accounts/{account_id}/start",
            post(post_browser_account_start),
        )
        .route(
            "/api/browsers/{account_id}/start",
            post(post_browser_account_start),
        )
        .route(
            "/api/browser/accounts/{account_id}/auth/start",
            post(post_browser_account_auth_start),
        )
        .route(
            "/api/browsers/{account_id}/auth/start",
            post(post_browser_account_auth_start),
        )
        .route(
            "/api/browser/accounts/{account_id}/auth/code",
            post(post_browser_account_auth_code),
        )
        .route(
            "/api/browsers/{account_id}/auth/code",
            post(post_browser_account_auth_code),
        )
        .route(
            "/api/browser/accounts/{account_id}/restart",
            post(post_browser_account_restart),
        )
        .route(
            "/api/browsers/{account_id}/restart",
            post(post_browser_account_restart),
        )
        .route(
            "/api/browser/accounts/{account_id}/stop",
            post(post_browser_account_stop),
        )
        .route(
            "/api/browsers/{account_id}/stop",
            post(post_browser_account_stop),
        )
        .route("/api/config/effective", get(get_effective_config))
        .route("/api/receipts/{run_id}", get(get_receipts))
        .route("/api/events", post(post_event))
        .route("/mcp", post(post_mcp))
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

async fn get_health(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Some(response) = browser_write_unauthorized(&state, &headers) {
        return response;
    }
    Json(json!({
        "status": "ok",
        "live": state.event_bus.is_some(),
        "runs": state.runs.read().await.len(),
        "browser_registry": state.browser_registry_path,
    }))
    .into_response()
}

async fn get_effective_config(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(state.config.redacted_for_display())
}

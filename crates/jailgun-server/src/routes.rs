use std::{net::SocketAddr, path::Path, sync::Arc};

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde_json::json;
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

use crate::{
    browser::{
        get_browser_account, get_browser_accounts, post_browser_account_auth_code,
        post_browser_account_auth_start, post_browser_account_restart, post_browser_account_start,
        post_browser_account_stop,
    },
    runs::{get_agent_summary, get_receipts, get_run, get_runs, post_event, start_agent_run},
    state::AppState,
    ws::ws_events,
};

pub fn api_router(state: AppState) -> Router {
    let state = Arc::new(state);
    Router::new()
        .route("/api/service", get(crate::control::status))
        .route("/api/service/stop", post(crate::control::stop))
        .route("/api/session/pairing-code", post(crate::auth::pairing_code))
        .route(
            "/api/tokens",
            get(crate::tokens::list).post(crate::tokens::issue),
        )
        .route("/api/tokens/{id}", delete(crate::tokens::revoke))
        .route(
            "/api/concept-runs",
            post(crate::concepts::submit).layer(axum::extract::DefaultBodyLimit::max(512 * 1024)),
        )
        .route("/api/accounts", get(crate::concepts::accounts))
        .route("/api/accounts/sessions", get(crate::accounts::sessions))
        .route("/api/accounts/connect", post(crate::accounts::connect))
        .route(
            "/api/accounts/{id}/reconnect",
            post(crate::accounts::reconnect),
        )
        .route("/api/accounts/{id}/confirm", post(crate::accounts::confirm))
        .route(
            "/api/accounts/{id}/login-view",
            get(crate::login_view::open),
        )
        .route(
            "/api/accounts/{id}/cancel-login",
            post(crate::accounts::cancel),
        )
        .route("/api/runs", get(get_runs).post(post(start_agent_run)))
        .route("/api/runs/{run_id}", get(get_run))
        .route("/api/runs/{run_id}/result", get(crate::concepts::result))
        .route("/api/runs/{run_id}/pause", post(crate::concepts::pause))
        .route("/api/runs/{run_id}/resume", post(crate::concepts::resume))
        .route("/api/runs/{run_id}/cancel", post(crate::concepts::cancel))
        .route(
            "/api/runs/{run_id}/attempts",
            get(crate::concepts::attempts),
        )
        .route("/api/runs/{run_id}/events", get(crate::concepts::events))
        .route(
            "/api/runs/{run_id}/artifacts/{artifact_id}",
            get(crate::concepts::artifact),
        )
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
        .nest_service("/mcp", crate::mcp::service(state.clone()))
        .route("/ws/events", get(ws_events))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::auth::protect,
        ))
        .route("/api/health", get(get_health))
        .route("/api/session/pair", post(crate::auth::pair))
        .with_state(state)
}

pub fn router_with_static(state: AppState, static_dir: impl AsRef<Path>) -> Router {
    api_router(state).fallback_service(ServeDir::new(static_dir.as_ref()))
}

pub async fn serve(addr: SocketAddr, router: Router) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, router).await
}

async fn get_health() -> Response {
    Json(json!({ "status": "ok" })).into_response()
}

async fn get_effective_config(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(state.config.redacted_for_display())
}

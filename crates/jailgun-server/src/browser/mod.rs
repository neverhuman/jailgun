mod auth;
mod registry;

pub(crate) use registry::browser_write_unauthorized;

use std::sync::Arc;

use axum::{
    extract::{Path as AxumPath, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use jailgun_orchestrator::bridge::{envelope_for_command, AuthSubmitCodePayload, BridgeCommand};
use serde_json::{json, Value};

use crate::{
    browser::{
        auth::{start_browser_auth_session, stop_browser_auth_session, AuthSessionMode},
        registry::{account_json, load_browser_registry},
    },
    state::{timestamp_now, AppState},
};

pub(crate) async fn get_browser_accounts(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = browser_write_unauthorized(&state, &headers) {
        return response;
    }
    match load_browser_registry(&state).await {
        Ok(registry) => Json(json!({
            "registry": state.browser_registry_path,
            "accounts": registry.accounts.into_iter().map(account_json).collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "browser-registry-read-failed", "reason": error.to_string() })),
        )
            .into_response(),
    }
}

pub(crate) async fn get_browser_account(
    State(state): State<Arc<AppState>>,
    AxumPath(account_id): AxumPath<String>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = browser_write_unauthorized(&state, &headers) {
        return response;
    }
    match load_browser_registry(&state).await {
        Ok(registry) => match registry.account(&account_id) {
            Some(account) => Json(account_json(account.clone())).into_response(),
            None => (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "browser-account-not-found", "account_id": account_id })),
            )
                .into_response(),
        },
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "browser-registry-read-failed", "reason": error.to_string() })),
        )
            .into_response(),
    }
}

pub(crate) async fn post_browser_account_start(
    State(state): State<Arc<AppState>>,
    AxumPath(account_id): AxumPath<String>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = browser_write_unauthorized(&state, &headers) {
        return response;
    }
    start_browser_auth_session(state, account_id, AuthSessionMode::Status).await
}

pub(crate) async fn post_browser_account_restart(
    State(state): State<Arc<AppState>>,
    AxumPath(account_id): AxumPath<String>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = browser_write_unauthorized(&state, &headers) {
        return response;
    }
    stop_browser_auth_session(&state, &account_id).await;
    start_browser_auth_session(state, account_id, AuthSessionMode::Status).await
}

pub(crate) async fn post_browser_account_stop(
    State(state): State<Arc<AppState>>,
    AxumPath(account_id): AxumPath<String>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = browser_write_unauthorized(&state, &headers) {
        return response;
    }
    stop_browser_auth_session(&state, &account_id).await;
    Json(json!({ "account_id": account_id, "status": "stopped" })).into_response()
}

pub(crate) async fn post_browser_account_auth_start(
    State(state): State<Arc<AppState>>,
    AxumPath(account_id): AxumPath<String>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = browser_write_unauthorized(&state, &headers) {
        return response;
    }
    start_browser_auth_session(state, account_id, AuthSessionMode::Begin).await
}

pub(crate) async fn post_browser_account_auth_code(
    State(state): State<Arc<AppState>>,
    AxumPath(account_id): AxumPath<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Some(response) = browser_write_unauthorized(&state, &headers) {
        return response;
    }
    let Some(code) = body.get("code").and_then(Value::as_str).map(str::trim) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "auth-code-required" })),
        )
            .into_response();
    };
    if code.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "auth-code-required" })),
        )
            .into_response();
    }

    let session = {
        let sessions = state.browser_auth_sessions.read().await;
        sessions.get(&account_id).cloned()
    };
    let Some(session) = session else {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": "auth-session-not-started", "account_id": account_id })),
        )
            .into_response();
    };
    let account = match load_browser_registry(&state).await {
        Ok(registry) => match registry.account(&account_id).cloned() {
            Some(account) => account,
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": "browser-account-not-found", "account_id": account_id })),
                )
                    .into_response();
            }
        },
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    json!({ "error": "browser-registry-read-failed", "reason": error.to_string() }),
                ),
            )
                .into_response();
        }
    };
    let command = BridgeCommand::AuthSubmitCode(AuthSubmitCodePayload {
        code: code.to_string(),
        profile_dir: Some(account.profile_dir.display().to_string()),
    });
    let run_id = format!("auth-{account_id}");
    match session
        .commands_tx
        .send(envelope_for_command(
            &command,
            run_id,
            timestamp_now(),
            None,
        ))
        .await
    {
        Ok(()) => Json(json!({ "account_id": account_id, "status": "submitted" })).into_response(),
        Err(_) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": "auth-session-closed", "account_id": account_id })),
        )
            .into_response(),
    }
}

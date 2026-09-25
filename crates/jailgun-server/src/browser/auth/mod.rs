mod bridge;
mod driver;

use std::sync::Arc;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use jailgun_core::BrowserAccountStatus;
use jailgun_orchestrator::bridge::{
    spawn_bridge, AuthBeginPayload, AuthStatusPayload, BridgeCommand, BridgeSpawnConfig,
    HelloPayload, ShutdownPayload, PROTOCOL_VERSION,
};
use serde_json::json;

use crate::{
    browser::{
        auth::{
            bridge::{
                browser_bridge_command, browser_bridge_env, send_server_bridge_command,
                wait_for_server_bridge_ready,
            },
            driver::drive_browser_auth_session,
        },
        registry::{account_json, load_browser_registry, update_browser_account_status},
    },
    state::{AppState, BrowserAuthSession},
};

#[derive(Clone, Copy)]
pub(super) enum AuthSessionMode {
    Status,
    Begin,
}

pub(super) async fn start_browser_auth_session(
    state: Arc<AppState>,
    account_id: String,
    mode: AuthSessionMode,
) -> Response {
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
    if let Err(error) = account.ensure_runtime_dirs() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "browser-runtime-dir-failed", "reason": error.to_string() })),
        )
            .into_response();
    }

    stop_browser_auth_session(&state, &account_id).await;
    let mut bridge = match spawn_bridge(BridgeSpawnConfig {
        command: match browser_bridge_command() {
            Ok(command) => command,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "browser-bridge-command-missing", "reason": error.to_string() })),
                )
                    .into_response();
            }
        },
        env: browser_bridge_env(&account),
    })
    .await
    {
        Ok(bridge) => bridge,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "browser-bridge-start-failed", "reason": error.to_string() })),
            )
                .into_response();
        }
    };

    let run_id = format!("auth-{account_id}");
    let hello = BridgeCommand::Hello(HelloPayload {
        orchestrator_version: env!("CARGO_PKG_VERSION").into(),
        protocol_version: PROTOCOL_VERSION,
        capabilities: vec!["auth-control-plane".into()],
    });
    if let Err(error) = send_server_bridge_command(&bridge.commands_tx, &run_id, hello).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "browser-bridge-send-failed", "reason": error.to_string() })),
        )
            .into_response();
    }
    if let Err(error) = wait_for_server_bridge_ready(&state, &mut bridge, &run_id).await {
        update_browser_account_status(&state, &account_id, BrowserAccountStatus::Degraded, None)
            .await
            .ok();
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "browser-bridge-not-ready", "reason": error.to_string() })),
        )
            .into_response();
    }

    if let Err(error) = send_server_bridge_command(
        &bridge.commands_tx,
        &run_id,
        auth_command(&state, &account, mode),
    )
    .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "browser-bridge-send-failed", "reason": error.to_string() })),
        )
            .into_response();
    }

    let commands_tx = bridge.commands_tx.clone();
    let session_id = uuid::Uuid::new_v4();
    state.browser_auth_sessions.write().await.insert(
        account_id.clone(),
        BrowserAuthSession {
            session_id,
            commands_tx,
        },
    );
    let state_for_task = state.clone();
    let account_for_task = account.clone();
    tokio::spawn(async move {
        drive_browser_auth_session(state_for_task, account_for_task, session_id, run_id, bridge)
            .await;
    });

    (
        StatusCode::ACCEPTED,
        Json(json!({
            "account": account_json(account),
            "status": match mode {
                AuthSessionMode::Status => "started",
                AuthSessionMode::Begin => "auth-started",
            },
        })),
    )
        .into_response()
}

pub(super) async fn stop_browser_auth_session(state: &Arc<AppState>, account_id: &str) {
    let session = state.browser_auth_sessions.write().await.remove(account_id);
    if let Some(session) = session {
        let run_id = format!("auth-{account_id}");
        let _ = send_server_bridge_command(
            &session.commands_tx,
            &run_id,
            BridgeCommand::Shutdown(ShutdownPayload {
                drain_timeout_ms: 1_000,
            }),
        )
        .await;
    }
}

fn auth_command(
    state: &AppState,
    account: &jailgun_core::BrowserAccount,
    mode: AuthSessionMode,
) -> BridgeCommand {
    match mode {
        AuthSessionMode::Status => BridgeCommand::AuthStatus(AuthStatusPayload {
            chat_url: state.config.browser.chat_url.clone(),
            profile_dir: Some(account.profile_dir.display().to_string()),
        }),
        AuthSessionMode::Begin => BridgeCommand::AuthBegin(AuthBeginPayload {
            chat_url: state.config.browser.chat_url.clone(),
            email_hint: account.email_hint.clone(),
            prefer_email_code: true,
            profile_dir: Some(account.profile_dir.display().to_string()),
        }),
    }
}

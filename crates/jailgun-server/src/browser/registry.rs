use std::sync::Arc;

use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use jailgun_core::{BrowserAccount, BrowserAccountStatus, BrowserProfileRegistry};
use serde_json::{json, Value};

use crate::state::AppState;

pub(crate) fn browser_write_unauthorized(
    state: &Arc<AppState>,
    headers: &HeaderMap,
) -> Option<Response> {
    let Some(expected) = state.ingest_token.as_deref() else {
        return Some(
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": "browser-control-token-required" })),
            )
                .into_response(),
        );
    };
    let provided = headers
        .get("x-jailgun-token")
        .and_then(|value| value.to_str().ok());
    if provided != Some(expected) {
        return Some(
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "browser-control-unauthorized" })),
            )
                .into_response(),
        );
    }
    None
}

pub(super) async fn load_browser_registry(
    state: &Arc<AppState>,
) -> Result<BrowserProfileRegistry, jailgun_core::BrowserRegistryError> {
    BrowserProfileRegistry::load_or_default(&state.browser_registry_path)
}

pub(super) async fn update_browser_account_status(
    state: &Arc<AppState>,
    account_id: &str,
    status: BrowserAccountStatus,
    last_verified_at: Option<String>,
) -> Result<(), jailgun_core::BrowserRegistryError> {
    let _guard = state.browser_registry_lock.lock().await;
    let mut registry = BrowserProfileRegistry::load_or_default(&state.browser_registry_path)?;
    if let Some(account) = registry.account_mut(account_id) {
        account.status = status;
        account.last_verified_at = last_verified_at;
    }
    registry.save(&state.browser_registry_path)
}

pub(super) fn account_json(account: BrowserAccount) -> Value {
    json!({
        "id": account.id,
        "email_hint": account.email_hint,
        "profile_dir": account.profile_dir,
        "state_dir": account.state_dir,
        "downloads_dir": account.downloads_dir,
        "cdp_port": account.cdp_port,
        "cdp_url": account.cdp_url(),
        "max_tabs": account.max_tabs,
        "status": account.status,
        "last_verified_at": account.last_verified_at,
    })
}

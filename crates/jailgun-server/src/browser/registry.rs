use std::sync::Arc;

use axum::{http::HeaderMap, response::Response};
use jailgun_core::{BrowserAccount, BrowserAccountStatus, BrowserProfileRegistry};
use serde_json::{json, Value};

use crate::state::AppState;

pub(crate) fn browser_write_unauthorized(
    state: &Arc<AppState>,
    headers: &HeaderMap,
) -> Option<Response> {
    if state.workflow.is_some() {
        return Some(crate::auth::error(
            axum::http::StatusCode::CONFLICT,
            "workflow-runtime-owned",
            "Use the concept account supervisor to manage this browser.",
        ));
    }
    crate::auth::unauthorized(state, headers)
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
    BrowserProfileRegistry::update(&state.browser_registry_path, |registry| {
        let account = registry.account_mut(account_id).ok_or_else(|| {
            jailgun_core::BrowserRegistryError::MissingAccount(account_id.to_string())
        })?;
        account.status = status;
        account.last_verified_at = last_verified_at;
        Ok(())
    })
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
        "provider_account_id": account.provider_account_id,
    })
}

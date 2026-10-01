use std::sync::Arc;

use jailgun_core::{BrowserAccount, BrowserAccountStatus, EventKind, JailgunEvent, Severity};
use jailgun_orchestrator::bridge::{BridgeEvent, BridgeHandle};

use crate::{
    browser::registry::update_browser_account_status,
    runs::record_event,
    state::{timestamp_now, AppState},
};

pub(super) async fn drive_browser_auth_session(
    state: Arc<AppState>,
    account: BrowserAccount,
    session_id: uuid::Uuid,
    run_id: String,
    mut bridge: BridgeHandle,
) {
    loop {
        let next = bridge.events_rx.recv().await;
        let envelope = match next {
            Some(Ok(envelope)) => envelope,
            Some(Err(error)) => {
                record_session_error(&state, &run_id, error.to_string()).await;
                continue;
            }
            None => break,
        };
        let event = match BridgeEvent::decode(&envelope.kind, envelope.payload) {
            Ok(event) => event,
            Err(error) => {
                record_session_error(&state, &run_id, error.to_string()).await;
                continue;
            }
        };
        if !auth_session_is_current(&state, &account.id, session_id).await {
            break;
        }
        if let Some(mapped) = jailgun_orchestrator::run::map_bridge_event(&run_id, None, &event) {
            publish_auth_event(&state, mapped).await;
        }
        apply_auth_event_status(&state, &account.id, &event).await;
    }
    if auth_session_is_current(&state, &account.id, session_id).await {
        state
            .browser_auth_sessions
            .write()
            .await
            .remove(&account.id);
    }
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), bridge.child.wait()).await;
}

async fn auth_session_is_current(
    state: &Arc<AppState>,
    account_id: &str,
    session_id: uuid::Uuid,
) -> bool {
    state
        .browser_auth_sessions
        .read()
        .await
        .get(account_id)
        .map(|session| session.session_id == session_id)
        .unwrap_or(false)
}

async fn record_session_error(state: &Arc<AppState>, run_id: &str, message: String) {
    let event = JailgunEvent::new(run_id.to_string(), EventKind::Error, message)
        .with_severity(Severity::Error);
    publish_auth_event(state, event).await;
}

async fn publish_auth_event(state: &Arc<AppState>, event: JailgunEvent) {
    record_event(state, event.clone()).await;
    if let Some(tx) = state.event_bus.as_ref() {
        let _ = tx.send(event);
    }
}

async fn apply_auth_event_status(state: &Arc<AppState>, account_id: &str, event: &BridgeEvent) {
    match event {
        BridgeEvent::AuthState(payload) => match payload.state.as_str() {
            "ready" => {} // Only AuthComplete with a verified identity can bind an account.
            "auth-required" | "code-requested" | "session-expired" => {
                set_account_status(state, account_id, BrowserAccountStatus::AuthRequired, None)
                    .await;
            }
            _ => {}
        },
        BridgeEvent::AuthComplete(payload) => {
            let result = match payload
                .account_identity
                .as_ref()
                .filter(|_| payload.composer_detected)
            {
                Some(identity) => jailgun_core::BrowserProfileRegistry::verify_account(
                    &state.browser_registry_path,
                    account_id,
                    identity,
                    timestamp_now(),
                ),
                None => Err(jailgun_core::BrowserRegistryError::AccountIdentityConflict(
                    account_id.into(),
                )),
            };
            if let Err(error) = result {
                set_account_status(state, account_id, BrowserAccountStatus::Degraded, None).await;
                record_session_error(state, &format!("auth-{account_id}"), error.to_string()).await;
            }
        }
        BridgeEvent::AuthActionNeeded(_) => {
            set_account_status(
                state,
                account_id,
                BrowserAccountStatus::ManualBrowserRequired,
                None,
            )
            .await;
        }
        BridgeEvent::AuthFailed(payload) => {
            let status = if payload.manual_browser_required {
                BrowserAccountStatus::ManualBrowserRequired
            } else {
                BrowserAccountStatus::Degraded
            };
            set_account_status(state, account_id, status, None).await;
        }
        BridgeEvent::SessionExpired(_) => {
            set_account_status(state, account_id, BrowserAccountStatus::AuthRequired, None).await;
        }
        _ => {}
    }
}

async fn set_account_status(
    state: &Arc<AppState>,
    account_id: &str,
    status: BrowserAccountStatus,
    authenticated_at: Option<String>,
) {
    let _ = update_browser_account_status(state, account_id, status, authenticated_at).await;
}

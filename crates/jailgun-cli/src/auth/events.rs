use std::time::Duration;

use anyhow::{Context, Result};
use jailgun_orchestrator::bridge::{BridgeEvent, BridgeHandle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AuthEvent {
    Complete,
    CodeRequested,
    ManualRequired(String),
    Failed(String),
}

pub(super) async fn wait_for_bridge_ready(
    bridge: &mut BridgeHandle,
    status_watch: bool,
) -> Result<()> {
    loop {
        match next_bridge_event(bridge).await? {
            BridgeEvent::BridgeReady(_) => return Ok(()),
            BridgeEvent::BridgeLog(payload) if status_watch => {
                eprintln!("bridge {}: {}", payload.phase, payload.message);
            }
            BridgeEvent::Error(payload) => {
                anyhow::bail!("bridge startup failed: {}", payload.message)
            }
            _ => {}
        }
    }
}

pub(super) async fn next_auth_event(
    bridge: &mut BridgeHandle,
    status_watch: bool,
) -> Result<AuthEvent> {
    loop {
        match next_bridge_event(bridge).await? {
            BridgeEvent::AuthState(payload) if status_watch => {
                eprintln!(
                    "auth state: {} ({})",
                    payload.state,
                    payload.reason.unwrap_or_default()
                );
            }
            BridgeEvent::AuthCodeRequested(payload) => {
                eprintln!(
                    "email verification code requested{}",
                    payload
                        .destination_hint
                        .as_deref()
                        .map(|hint| format!(": {hint}"))
                        .unwrap_or_default()
                );
                return Ok(AuthEvent::CodeRequested);
            }
            BridgeEvent::AuthComplete(_) => return Ok(AuthEvent::Complete),
            BridgeEvent::AuthActionNeeded(payload) => {
                return Ok(AuthEvent::ManualRequired(payload.reason));
            }
            BridgeEvent::AuthFailed(payload) => {
                if payload.manual_browser_required {
                    return Ok(AuthEvent::ManualRequired(payload.reason));
                }
                return Ok(AuthEvent::Failed(payload.reason));
            }
            BridgeEvent::SessionExpired(payload) => {
                return Ok(AuthEvent::ManualRequired(payload.reason));
            }
            BridgeEvent::Error(payload) => return Ok(AuthEvent::Failed(payload.message)),
            BridgeEvent::BridgeLog(payload) if status_watch => {
                eprintln!("bridge {}: {}", payload.phase, payload.message);
            }
            _ => {}
        }
    }
}

async fn next_bridge_event(bridge: &mut BridgeHandle) -> Result<BridgeEvent> {
    let envelope = tokio::time::timeout(Duration::from_secs(180), bridge.events_rx.recv())
        .await
        .context("timed out waiting for chrome-bridge auth event")?
        .context("chrome-bridge exited before auth completed")?
        .context("chrome-bridge protocol error")?;
    BridgeEvent::decode(&envelope.kind, envelope.payload).context("decoding chrome-bridge event")
}

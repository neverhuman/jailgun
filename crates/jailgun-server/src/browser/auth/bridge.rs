use std::{collections::BTreeMap, env, path::PathBuf, sync::Arc};

use anyhow::Context;
use jailgun_core::BrowserAccount;
use jailgun_orchestrator::bridge::{
    envelope_for_command, BridgeCommand, BridgeEvent, BridgeHandle, Envelope,
};
use tokio::sync::mpsc;

use crate::{
    runs::record_event,
    state::{timestamp_now, AppState},
};

pub(super) fn browser_bridge_command() -> anyhow::Result<Vec<String>> {
    if let Ok(value) = env::var("JAILGUN_BRIDGE_CMD") {
        let parts = value
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>();
        if !parts.is_empty() {
            return Ok(parts);
        }
    }
    let workspace_script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/chrome-bridge/bin/chrome-bridge.mjs");
    if workspace_script.exists() {
        return Ok(vec!["node".into(), workspace_script.display().to_string()]);
    }
    anyhow::bail!("JAILGUN_BRIDGE_CMD is required for browser control-plane endpoints")
}

pub(super) fn browser_bridge_env(account: &BrowserAccount) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "JAILGUN_CHROME_PROFILE_DIR".into(),
            account.profile_dir.display().to_string(),
        ),
        (
            "JAILGUN_CHROME_STATE_DIR".into(),
            account.state_dir.display().to_string(),
        ),
        (
            "JAILGUN_DOWNLOADS_DIR".into(),
            account.downloads_dir.display().to_string(),
        ),
        ("JAILGUN_CDP_HOST".into(), "127.0.0.1".into()),
        ("JAILGUN_CDP_PORT".into(), account.cdp_port.to_string()),
        (
            "JAILGUN_CHROME_PROFILE_POOL".into(),
            format!("{}={}", account.id, account.profile_dir.display()),
        ),
        (
            "JAILGUN_CHROME_PROFILE_PORTS".into(),
            format!("{}={}", account.id, account.cdp_port),
        ),
    ])
}

pub(super) async fn send_server_bridge_command(
    commands_tx: &mpsc::Sender<Envelope<serde_json::Value>>,
    run_id: &str,
    command: BridgeCommand,
) -> anyhow::Result<()> {
    commands_tx
        .send(envelope_for_command(
            &command,
            run_id,
            timestamp_now(),
            None,
        ))
        .await
        .map_err(|_| anyhow::anyhow!("chrome-bridge command channel is closed"))
}

pub(super) async fn wait_for_server_bridge_ready(
    state: &Arc<AppState>,
    bridge: &mut BridgeHandle,
    run_id: &str,
) -> anyhow::Result<()> {
    loop {
        let envelope =
            tokio::time::timeout(std::time::Duration::from_secs(90), bridge.events_rx.recv())
                .await
                .context("timed out waiting for chrome-bridge ready")?
                .context("chrome-bridge exited before ready")?
                .context("chrome-bridge protocol error before ready")?;
        let event = BridgeEvent::decode(&envelope.kind, envelope.payload)
            .context("decoding chrome-bridge event before ready")?;
        match event {
            BridgeEvent::BridgeReady(_) => return Ok(()),
            BridgeEvent::Error(payload) => {
                anyhow::bail!("chrome-bridge startup failed: {}", payload.message)
            }
            other => {
                if let Some(mapped) =
                    jailgun_orchestrator::run::map_bridge_event(run_id, None, &other)
                {
                    record_event(state, mapped.clone()).await;
                    if let Some(tx) = state.event_bus.as_ref() {
                        let _ = tx.send(mapped);
                    }
                }
            }
        }
    }
}

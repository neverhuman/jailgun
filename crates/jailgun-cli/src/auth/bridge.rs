use std::collections::BTreeMap;

use anyhow::{Context, Result};
use jailgun_core::BrowserAccount;
use jailgun_orchestrator::bridge::{envelope_for_command, BridgeCommand, BridgeHandle};

use super::timestamp_now;

pub(super) fn auth_bridge_command(args: Vec<String>) -> Result<Vec<String>> {
    jailgun_orchestrator::support::bridge_command(args)
}

pub(super) fn parse_env_overrides(values: Vec<String>) -> Result<BTreeMap<String, String>> {
    let mut envs = BTreeMap::new();
    for value in values {
        let Some((key, val)) = value.split_once('=') else {
            anyhow::bail!("--bridge-env must be KEY=VALUE, got {value:?}");
        };
        if key.trim().is_empty() {
            anyhow::bail!("--bridge-env key cannot be empty");
        }
        envs.insert(key.to_string(), val.to_string());
    }
    Ok(envs)
}

pub(super) fn account_bridge_env(
    account: &BrowserAccount,
    bridge_env: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut env = bridge_env.clone();
    env.insert(
        "JAILGUN_CHROME_PROFILE_DIR".into(),
        account.profile_dir.display().to_string(),
    );
    env.insert(
        "JAILGUN_CHROME_STATE_DIR".into(),
        account.state_dir.display().to_string(),
    );
    env.insert(
        "JAILGUN_DOWNLOADS_DIR".into(),
        account.downloads_dir.display().to_string(),
    );
    env.insert("JAILGUN_CDP_HOST".into(), "127.0.0.1".into());
    env.insert("JAILGUN_CDP_PORT".into(), account.cdp_port.to_string());
    env.insert(
        "JAILGUN_CHROME_PROFILE_POOL".into(),
        format!("{}={}", account.id, account.profile_dir.display()),
    );
    env
}

pub(super) async fn send_bridge_command(
    bridge: &BridgeHandle,
    run_id: &str,
    command: BridgeCommand,
) -> Result<()> {
    bridge
        .commands_tx
        .send(envelope_for_command(
            &command,
            run_id,
            timestamp_now(),
            None,
        ))
        .await
        .context("sending auth command to chrome-bridge")
}

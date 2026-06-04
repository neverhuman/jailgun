use std::{collections::BTreeMap, env, path::PathBuf};

use anyhow::{Context, Result};
use jailgun_core::BrowserAccount;
use jailgun_orchestrator::bridge::{envelope_for_command, BridgeCommand, BridgeHandle};

use super::timestamp_now;

pub(super) fn auth_bridge_command(args: Vec<String>) -> Result<Vec<String>> {
    if !args.is_empty() {
        return Ok(args);
    }
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
    anyhow::bail!("bridge command must be provided with --bridge-cmd or JAILGUN_BRIDGE_CMD")
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

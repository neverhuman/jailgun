use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::Result;
use jailgun_core::{BrowserAccount, JailgunConfig};

use crate::{
    agent::accounts::{
        join_account_profile_pool, join_account_profile_ports, join_path_profile_pool,
    },
    support::default_managed_chrome_state_dir,
};

pub(super) fn apply_profile_env(
    bridge_env: &mut BTreeMap<String, String>,
    config: &JailgunConfig,
    account_profiles: &[BrowserAccount],
    explicit_profile_pool: &[PathBuf],
    profile_dir: &Path,
) -> Result<()> {
    if !account_profiles.is_empty() {
        apply_account_profile_env(bridge_env, config, account_profiles)?;
    } else if !explicit_profile_pool.is_empty() {
        bridge_env.insert(
            "JAILGUN_CHROME_PROFILE_POOL".into(),
            join_path_profile_pool(explicit_profile_pool)?,
        );
        apply_default_profile_env(bridge_env, config, profile_dir);
    } else {
        apply_default_profile_env(bridge_env, config, profile_dir);
    }
    Ok(())
}

pub(super) fn clear_profile_routing_env(
    bridge_env: &mut BTreeMap<String, String>,
    config: &JailgunConfig,
) {
    for key in [
        "JAILGUN_CHROME_PROFILE_POOL",
        "JAILGUN_CHROME_PROFILE_DIRS",
        "JAILGUN_CHROME_PROFILE_PORTS",
        "JAILGUN_CDP_URL",
        "JAILGUN_CDP_HOST",
        "JAILGUN_CDP_PORT",
        "GOOGLE_AUTOMATION_REMOTE_DEBUG_HOST",
        "GOOGLE_AUTOMATION_REMOTE_DEBUG_PORT",
        config.browser.profile_dir_env.as_str(),
        config.browser.state_dir_env.as_str(),
    ] {
        bridge_env.remove(key);
    }
}

fn apply_account_profile_env(
    bridge_env: &mut BTreeMap<String, String>,
    config: &JailgunConfig,
    account_profiles: &[BrowserAccount],
) -> Result<()> {
    let primary = &account_profiles[0];
    let primary_cdp_port = primary.cdp_port.to_string();
    bridge_env.insert(
        "JAILGUN_CHROME_PROFILE_POOL".into(),
        join_account_profile_pool(account_profiles)?,
    );
    bridge_env.insert(
        "JAILGUN_CHROME_PROFILE_PORTS".into(),
        join_account_profile_ports(account_profiles)?,
    );
    bridge_env.insert(
        "JAILGUN_CDP_URL".into(),
        format!("http://127.0.0.1:{primary_cdp_port}"),
    );
    bridge_env.insert("JAILGUN_CDP_HOST".into(), "127.0.0.1".into());
    bridge_env.insert("JAILGUN_CDP_PORT".into(), primary_cdp_port.clone());
    bridge_env.insert(
        "GOOGLE_AUTOMATION_REMOTE_DEBUG_HOST".into(),
        "127.0.0.1".into(),
    );
    bridge_env.insert(
        "GOOGLE_AUTOMATION_REMOTE_DEBUG_PORT".into(),
        primary_cdp_port,
    );
    bridge_env.insert(
        config.browser.profile_dir_env.clone(),
        primary.profile_dir.display().to_string(),
    );
    bridge_env.insert(
        config.browser.state_dir_env.clone(),
        primary.state_dir.display().to_string(),
    );
    Ok(())
}

fn apply_default_profile_env(
    bridge_env: &mut BTreeMap<String, String>,
    config: &JailgunConfig,
    profile_dir: &Path,
) {
    bridge_env
        .entry(config.browser.profile_dir_env.clone())
        .or_insert_with(|| profile_dir.display().to_string());
    bridge_env
        .entry(config.browser.state_dir_env.clone())
        .or_insert_with(|| default_managed_chrome_state_dir().display().to_string());
}

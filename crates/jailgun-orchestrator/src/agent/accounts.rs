use std::{collections::BTreeSet, path::PathBuf};

use anyhow::{Context, Result};
use jailgun_core::{
    validate_account_id, BrowserAccount, BrowserProfileRegistry, JailgunAgentRunRequest,
    JailgunConfig,
};

pub(super) fn resolve_requested_accounts(
    request: &JailgunAgentRunRequest,
    config: &JailgunConfig,
    requested_tabs: u16,
) -> Result<Vec<BrowserAccount>> {
    if request.prompt_ref.starts_with("jmcp://")
        && request.browser.account_ids.is_empty()
        && (request.browser.profile_dir.is_some() || !request.browser.profile_pool.is_empty())
    {
        anyhow::bail!(
            "jmcp:// runs must use ready browser.account_ids instead of raw profile paths"
        );
    }
    let should_use_registry = !request.browser.account_ids.is_empty()
        || (request.prompt_ref.starts_with("jmcp://")
            && request.browser.profile_dir.is_none()
            && request.browser.profile_pool.is_empty());
    if !should_use_registry {
        return Ok(Vec::new());
    }
    let registry_path = browser_registry_path_for_request(request, config);
    let registry = BrowserProfileRegistry::load_or_default(&registry_path).with_context(|| {
        format!(
            "loading browser profile registry {}",
            registry_path.display()
        )
    })?;
    let mut accounts = Vec::new();
    if request.browser.account_ids.is_empty() {
        accounts.extend(
            registry
                .accounts
                .iter()
                .filter(|account| account.status == jailgun_core::BrowserAccountStatus::Ready)
                .cloned(),
        );
        if accounts.is_empty() {
            anyhow::bail!(
                "jmcp:// runs require at least one ready browser account in {}",
                registry_path.display()
            );
        }
    } else {
        let mut seen = BTreeSet::new();
        for id in &request.browser.account_ids {
            validate_account_id(id).map_err(anyhow::Error::new)?;
            if !seen.insert(id.clone()) {
                anyhow::bail!("duplicate browser account id requested: {id}");
            }
            let account = registry
                .require_account(id)
                .with_context(|| format!("resolving browser account {id}"))?;
            account.require_ready()?;
            accounts.push(account.clone());
        }
    }
    ensure_account_capacity(&accounts, requested_tabs)?;
    Ok(accounts)
}

pub(super) fn browser_registry_path_for_request(
    request: &JailgunAgentRunRequest,
    config: &JailgunConfig,
) -> PathBuf {
    request
        .browser
        .bridge_env
        .get(&config.browser.profile_registry_env)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            BrowserProfileRegistry::default_path_from_env(&config.browser.profile_registry_env)
        })
}

pub(super) fn ensure_account_capacity(
    accounts: &[BrowserAccount],
    requested_tabs: u16,
) -> Result<()> {
    let capacity = accounts
        .iter()
        .map(|account| account.max_tabs.max(1))
        .fold(0u16, u16::saturating_add);
    if requested_tabs > capacity {
        anyhow::bail!(
            "requested {requested_tabs} tab(s), but selected browser accounts allow {capacity} tab(s)"
        );
    }
    Ok(())
}

pub(super) fn join_account_profile_pool(accounts: &[BrowserAccount]) -> Result<String> {
    let entries = accounts
        .iter()
        .map(|account| format!("{}={}", account.id, account.profile_dir.display()))
        .collect::<Vec<_>>();
    join_profile_pool_entries(&entries)
}

pub(super) fn join_account_profile_ports(accounts: &[BrowserAccount]) -> Result<String> {
    let entries = accounts
        .iter()
        .map(|account| format!("{}={}", account.id, account.cdp_port))
        .collect::<Vec<_>>();
    join_profile_pool_entries(&entries)
}

pub(super) fn join_path_profile_pool(paths: &[PathBuf]) -> Result<String> {
    let entries = paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    join_profile_pool_entries(&entries)
}

fn join_profile_pool_entries(entries: &[String]) -> Result<String> {
    let separator = profile_pool_separator();
    for entry in entries {
        if entry.contains(separator) {
            anyhow::bail!("browser profile pool entry contains path-list separator: {entry}");
        }
    }
    Ok(entries.join(&separator.to_string()))
}

fn profile_pool_separator() -> char {
    if cfg!(windows) {
        ';'
    } else {
        ':'
    }
}

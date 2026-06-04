mod bridge;
mod events;
mod session;

use std::path::PathBuf;

use anyhow::{Context, Result};
use jailgun_core::{
    BrowserAccountRoots, BrowserAccountStatus, BrowserProfileRegistry,
    DEFAULT_BROWSER_REGISTRY_ENV, JAILGUN_AGENT_MAX_TABS,
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use self::{
    bridge::{auth_bridge_command, parse_env_overrides},
    session::setup_one_account,
};

pub struct AuthSetupOptions {
    pub emails: Vec<String>,
    pub id: Option<String>,
    pub registry: Option<PathBuf>,
    pub profile_root: Option<PathBuf>,
    pub state_root: Option<PathBuf>,
    pub downloads_root: Option<PathBuf>,
    pub cdp_port_start: u16,
    pub prefer_email_code: bool,
    pub code_stdin: bool,
    pub status_watch: bool,
    pub bridge_cmd: Vec<String>,
    pub bridge_env: Vec<String>,
}

pub async fn setup(options: AuthSetupOptions) -> Result<()> {
    if options.emails.len() > 1 && options.id.is_some() {
        anyhow::bail!("--id can only be used with one --email");
    }

    let registry_path = options.registry.clone().unwrap_or_else(|| {
        BrowserProfileRegistry::default_path_from_env(DEFAULT_BROWSER_REGISTRY_ENV)
    });
    let mut registry = BrowserProfileRegistry::load_or_default(&registry_path)
        .with_context(|| format!("loading browser registry {}", registry_path.display()))?;
    let roots = account_roots(&options);
    let bridge_cmd = auth_bridge_command(options.bridge_cmd.clone())?;
    let bridge_env = parse_env_overrides(options.bridge_env.clone())?;

    let mut results = Vec::new();
    for (index, email) in options.emails.iter().enumerate() {
        let cdp_port = options
            .cdp_port_start
            .checked_add(index as u16)
            .context("cdp port allocation overflowed")?;
        let account = registry.upsert_account(
            email,
            options.id.clone(),
            &roots,
            cdp_port,
            JAILGUN_AGENT_MAX_TABS,
        )?;
        registry.save(&registry_path)?;

        let outcome = setup_one_account(
            &account,
            &bridge_cmd,
            &bridge_env,
            options.prefer_email_code,
            options.code_stdin,
            options.status_watch,
        )
        .await;

        match outcome {
            Ok(()) => {
                if let Some(stored) = registry.account_mut(&account.id) {
                    stored.status = BrowserAccountStatus::Ready;
                    stored.last_verified_at = Some(timestamp_now());
                }
                results.push(serde_json::json!({
                    "id": account.id,
                    "email_hint": account.email_hint,
                    "status": "ready",
                    "profile_dir": account.profile_dir,
                    "state_dir": account.state_dir,
                    "downloads_dir": account.downloads_dir,
                    "cdp_url": account.cdp_url(),
                }));
            }
            Err(error) => {
                if let Some(stored) = registry.account_mut(&account.id) {
                    stored.status = status_for_error(&error);
                    stored.last_verified_at = None;
                }
                registry.save(&registry_path)?;
                anyhow::bail!(
                    "auth setup failed for {} ({}): {error}",
                    account.id,
                    account.email_hint
                );
            }
        }
        registry.save(&registry_path)?;
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "registry": registry_path,
            "accounts": results,
        }))?
    );
    Ok(())
}

fn account_roots(options: &AuthSetupOptions) -> BrowserAccountRoots {
    let defaults = BrowserAccountRoots::default_under_home();
    BrowserAccountRoots {
        profile_root: options
            .profile_root
            .clone()
            .unwrap_or(defaults.profile_root),
        state_root: options.state_root.clone().unwrap_or(defaults.state_root),
        downloads_root: options
            .downloads_root
            .clone()
            .unwrap_or(defaults.downloads_root),
    }
}

fn status_for_error(error: &anyhow::Error) -> BrowserAccountStatus {
    if error.to_string().contains("manual-browser-required") {
        BrowserAccountStatus::ManualBrowserRequired
    } else {
        BrowserAccountStatus::Degraded
    }
}

fn timestamp_now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

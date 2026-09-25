use super::*;

pub(super) struct BrowserWorkflowResult {
    pub(super) summary: RunSummary,
    pub(super) downloaded: Vec<PathBuf>,
}

pub(super) async fn run_browser_workflow(opts: RunOptions) -> Result<BrowserWorkflowResult> {
    let mut handle = run_orchestration(opts).await?;
    let mut events_open = true;
    let mut downloaded = Vec::new();
    let summary = loop {
        tokio::select! {
            event = handle.events_rx.recv(), if events_open => {
                match event {
                    Ok(event) => {
                        println!("{}", serde_json::to_string(&event)?);
                        if matches!(event.kind, EventKind::DownloadReceipt) {
                            if let Some(path) = event.fields.get("local_path") {
                                downloaded.push(PathBuf::from(path));
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(dropped)) => {
                        eprintln!("event stream lagged; dropped {dropped} event(s)");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        events_open = false;
                    }
                }
            }
            summary = &mut handle.completion => {
                let summary = summary.context("orchestrator task ended before sending a summary")?;
                println!(
                    "{}",
                    serde_json::to_string(&json!({
                        "type": "run-summary",
                        "summary": summary,
                    }))?
                );
                break summary;
            }
        }
    };
    let _ = handle.shutdown.send(true);
    Ok(BrowserWorkflowResult {
        summary,
        downloaded,
    })
}

pub(super) fn resolve_accounts(
    config: &JailgunConfig,
    requested: &[String],
) -> Result<Vec<BrowserAccount>> {
    let registry_path =
        BrowserProfileRegistry::default_path_from_env(&config.browser.profile_registry_env);
    let registry = BrowserProfileRegistry::load_or_default(&registry_path).with_context(|| {
        format!(
            "loading browser profile registry {}",
            registry_path.display()
        )
    })?;
    let accounts = if requested.is_empty() {
        registry
            .accounts
            .iter()
            .filter(|account| account.status == BrowserAccountStatus::Ready)
            .cloned()
            .collect::<Vec<_>>()
    } else {
        let mut seen = BTreeSet::new();
        let mut selected = Vec::new();
        for id in requested {
            validate_account_id(id).map_err(anyhow::Error::new)?;
            if !seen.insert(id.clone()) {
                anyhow::bail!("duplicate browser account id requested: {id}");
            }
            let account = registry
                .require_account(id)
                .with_context(|| format!("resolving browser account {id}"))?;
            account.require_ready()?;
            selected.push(account.clone());
        }
        selected
    };
    if accounts.is_empty() {
        anyhow::bail!(
            "jailhard requires at least one ready browser account in {} (env: {})",
            registry_path.display(),
            config.browser.profile_registry_env
        );
    }
    Ok(accounts)
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

pub(super) fn apply_account_profile_env(
    bridge_env: &mut BTreeMap<String, String>,
    config: &JailgunConfig,
    accounts: &[BrowserAccount],
) -> Result<()> {
    let primary = accounts
        .first()
        .context("at least one browser account is required")?;
    let primary_cdp_port = primary.cdp_port.to_string();
    bridge_env.insert(
        "JAILGUN_CHROME_PROFILE_POOL".into(),
        join_profile_pool(
            accounts
                .iter()
                .map(|account| format!("{}={}", account.id, account.profile_dir.display())),
        )?,
    );
    bridge_env.insert(
        "JAILGUN_CHROME_PROFILE_PORTS".into(),
        join_profile_pool(
            accounts
                .iter()
                .map(|account| format!("{}={}", account.id, account.cdp_port)),
        )?,
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

pub(super) fn join_profile_pool(entries: impl Iterator<Item = String>) -> Result<String> {
    let separator = if cfg!(windows) { ';' } else { ':' };
    let entries = entries.collect::<Vec<_>>();
    for entry in &entries {
        if entry.contains(separator) {
            anyhow::bail!("browser profile pool entry contains path-list separator: {entry}");
        }
    }
    Ok(entries.join(&separator.to_string()))
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

use std::{collections::BTreeMap, fs, time::Duration};

use anyhow::{Context, Result};
use jailgun_core::{
    BrowserLease, BrowserLeaseManager, BrowserRegistryError, EventKind, JailgunAgentRunSummary,
    JailgunEvent, Severity,
};
use tokio::io::AsyncWriteExt;

use crate::{
    agent::{
        execute_summary::build_agent_summary, prepare_env::apply_profile_env, timestamp_now,
        AgentRunBackend, AgentRunEventSink, PreparedAgentRun,
    },
    run::{OrchestratorHandle, RunSummary},
    support::ensure_parent_dir,
};

pub async fn execute_prepared_agent_run(
    mut prepared: PreparedAgentRun,
    backend: &dyn AgentRunBackend,
    sink: &dyn AgentRunEventSink,
) -> Result<JailgunAgentRunSummary> {
    ensure_parent_dir(&prepared.output_paths.events_jsonl)?;
    let mut file = tokio::fs::File::create(&prepared.output_paths.events_jsonl)
        .await
        .with_context(|| format!("creating {}", prepared.output_paths.events_jsonl.display()))?;
    let mut events = Vec::new();
    let mut browser_lease =
        acquire_browser_lease(&mut prepared, &mut file, &mut events, sink).await?;
    let handle = match backend.start(prepared.opts.clone()).await {
        Ok(handle) => handle,
        Err(error) => {
            release_browser_lease(&prepared, &mut browser_lease, &mut file, &mut events, sink)
                .await?;
            return Err(error);
        }
    };
    let collection_result = collect_agent_run_events(
        handle,
        file,
        events,
        &prepared.opts.run_id,
        prepared.tabs,
        prepared.max_runtime_seconds,
        sink,
    )
    .await;
    let mut collection = match collection_result {
        Ok(collection) => collection,
        Err(error) => {
            if let Some(mut lease) = browser_lease.take() {
                lease
                    .release()
                    .context("releasing browser account lease after event collection failure")?;
            }
            return Err(error);
        }
    };
    let mut file = collection
        .file
        .take()
        .expect("collection retains event file");
    release_browser_lease(
        &prepared,
        &mut browser_lease,
        &mut file,
        &mut collection.events,
        sink,
    )
    .await?;
    file.flush().await?;
    let finished_at = timestamp_now();
    let summary = build_agent_summary(
        &prepared.request,
        &prepared.config,
        &prepared.repo_url,
        &prepared.output_paths.events_jsonl,
        prepared.started_at,
        finished_at,
        prepared.tabs,
        prepared.max_runtime_seconds,
        prepared.deploy_expected_top_level.as_deref(),
        collection,
    );
    ensure_parent_dir(&prepared.output_paths.summary_json)?;
    fs::write(
        &prepared.output_paths.summary_json,
        serde_json::to_vec_pretty(&summary)?,
    )
    .with_context(|| format!("writing {}", prepared.output_paths.summary_json.display()))?;
    sink.on_summary(&summary).await?;
    Ok(summary)
}

pub(super) struct AgentRunCollection {
    pub(super) summary: RunSummary,
    pub(super) events: Vec<JailgunEvent>,
    pub(super) timed_out: bool,
    file: Option<tokio::fs::File>,
}

async fn collect_agent_run_events(
    mut handle: OrchestratorHandle,
    mut file: tokio::fs::File,
    mut events: Vec<JailgunEvent>,
    run_id: &str,
    tabs: u16,
    max_runtime_seconds: u64,
    sink: &dyn AgentRunEventSink,
) -> Result<AgentRunCollection> {
    let mut events_open = true;
    let deadline = tokio::time::sleep(std::time::Duration::from_secs(max_runtime_seconds));
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            _ = &mut deadline => {
                let _ = handle.shutdown.send(true);
                file.flush().await?;
                return Ok(AgentRunCollection {
                    summary: RunSummary {
                        run_id: run_id.to_string(),
                        total_tabs: tabs,
                        downloaded: 0,
                        deployed: 0,
                        failures: vec![(0, "agent max runtime exceeded".into())],
                        denied_github_prompts: 0,
                        allowed_info_prompts: 0,
                    },
                    events,
                    timed_out: true,
                    file: Some(file),
                });
            }
            event = handle.events_rx.recv(), if events_open => {
                match event {
                    Ok(event) => {
                        write_agent_event(&mut file, &mut events, sink, event).await?;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(dropped)) => {
                        eprintln!("event stream lagged; dropped {dropped} event(s)");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        events_open = false;
                    }
                }
            }
            summary = &mut handle.completion => {
                let summary = summary.context("orchestrator task ended before sending a summary")?;
                file.flush().await?;
                let _ = handle.shutdown.send(true);
                return Ok(AgentRunCollection {
                    summary,
                    events,
                    timed_out: false,
                    file: Some(file),
                });
            }
        }
    }
}

async fn acquire_browser_lease(
    prepared: &mut PreparedAgentRun,
    file: &mut tokio::fs::File,
    events: &mut Vec<JailgunEvent>,
    sink: &dyn AgentRunEventSink,
) -> Result<Option<BrowserLease>> {
    let Some(spec) = prepared.browser_lease.clone() else {
        return Ok(None);
    };
    let manager = BrowserLeaseManager::new(spec.registry_path.clone());
    let queue_timeout = spec.request.effective_queue_timeout_seconds();
    let queued = JailgunEvent::new(
        prepared.opts.run_id.clone(),
        EventKind::RunQueued,
        "browser account capacity queued",
    )
    .with_field("requested_tabs", spec.request.tabs.to_string())
    .with_field(
        "requested_account_count",
        spec.request.account_ids.len().to_string(),
    )
    .with_field("allow_queueing", spec.request.allow_queueing.to_string())
    .with_field("queue_timeout_seconds", queue_timeout.to_string());
    write_agent_event(file, events, sink, queued).await?;

    let started = tokio::time::Instant::now();
    loop {
        match manager.try_acquire(&spec.request) {
            Ok(lease) => {
                apply_browser_lease_to_options(prepared, &lease)?;
                let acquired = JailgunEvent::new(
                    prepared.opts.run_id.clone(),
                    EventKind::BrowserLeaseAcquired,
                    "browser account lease acquired",
                )
                .with_field("leased_account_count", lease.accounts().len().to_string())
                .with_field("leased_tab_count", lease.tab_accounts().len().to_string())
                .with_field("queue_wait_ms", started.elapsed().as_millis().to_string());
                write_agent_event(file, events, sink, acquired).await?;
                return Ok(Some(lease));
            }
            Err(BrowserRegistryError::LeaseBusy { .. }) if spec.request.allow_queueing => {
                if started.elapsed() >= Duration::from_secs(queue_timeout) {
                    let message = "browser account queue timed out";
                    let event =
                        JailgunEvent::new(prepared.opts.run_id.clone(), EventKind::Error, message)
                            .with_severity(Severity::Error)
                            .with_field("kind", "browser-lease-timeout")
                            .with_field("requested_tabs", spec.request.tabs.to_string())
                            .with_field("queue_timeout_seconds", queue_timeout.to_string());
                    write_agent_event(file, events, sink, event).await?;
                    anyhow::bail!("{message}");
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(error) => {
                let code = browser_lease_error_code(&error);
                let event = JailgunEvent::new(
                    prepared.opts.run_id.clone(),
                    EventKind::Error,
                    format!("browser lease failed: {code}"),
                )
                .with_severity(Severity::Error)
                .with_field("kind", code)
                .with_field("requested_tabs", spec.request.tabs.to_string());
                write_agent_event(file, events, sink, event).await?;
                anyhow::bail!("browser lease failed: {code}");
            }
        }
    }
}

fn apply_browser_lease_to_options(
    prepared: &mut PreparedAgentRun,
    lease: &BrowserLease,
) -> Result<()> {
    let Some(primary) = lease.accounts().first() else {
        anyhow::bail!("browser lease returned no accounts");
    };
    apply_profile_env(
        &mut prepared.opts.bridge_env,
        &prepared.opts.config,
        lease.accounts(),
        &[],
        &primary.profile_dir,
    )?;
    prepared.opts.profile_dir = primary.profile_dir.clone();
    prepared.opts.profile_pool = lease
        .accounts()
        .iter()
        .map(|account| account.profile_dir.clone())
        .collect();
    prepared.opts.tab_profile_dirs = lease
        .tab_accounts()
        .iter()
        .enumerate()
        .map(|(index, account)| ((index as u16) + 1, account.profile_dir.clone()))
        .collect::<BTreeMap<_, _>>();
    Ok(())
}

async fn release_browser_lease(
    prepared: &PreparedAgentRun,
    lease: &mut Option<BrowserLease>,
    file: &mut tokio::fs::File,
    events: &mut Vec<JailgunEvent>,
    sink: &dyn AgentRunEventSink,
) -> Result<()> {
    let Some(lease) = lease.as_mut() else {
        return Ok(());
    };
    let leased_account_count = lease.accounts().len();
    let leased_tab_count = lease.tab_accounts().len();
    lease.release().context("releasing browser account lease")?;
    let released = JailgunEvent::new(
        prepared.opts.run_id.clone(),
        EventKind::BrowserLeaseReleased,
        "browser account lease released",
    )
    .with_field("leased_account_count", leased_account_count.to_string())
    .with_field("leased_tab_count", leased_tab_count.to_string());
    write_agent_event(file, events, sink, released).await
}

async fn write_agent_event(
    file: &mut tokio::fs::File,
    events: &mut Vec<JailgunEvent>,
    sink: &dyn AgentRunEventSink,
    event: JailgunEvent,
) -> Result<()> {
    file.write_all(&serde_json::to_vec(&event)?).await?;
    file.write_all(b"\n").await?;
    sink.on_event(&event).await?;
    events.push(event);
    Ok(())
}

fn browser_lease_error_code(error: &BrowserRegistryError) -> &'static str {
    match error {
        BrowserRegistryError::MissingAccount(_) => "browser-account-not-registered",
        BrowserRegistryError::AccountNotReady { .. } => "browser-account-not-ready",
        BrowserRegistryError::NoReadyAccounts => "browser-account-none-ready",
        BrowserRegistryError::InsufficientAccountCapacity { .. } => {
            "browser-account-insufficient-capacity"
        }
        BrowserRegistryError::LeaseBusy { .. } | BrowserRegistryError::LeaseUnavailable { .. } => {
            "browser-account-capacity-busy"
        }
        BrowserRegistryError::Lock { .. } => "browser-lease-lock-failed",
        BrowserRegistryError::Read { .. } => "browser-registry-read-failed",
        BrowserRegistryError::Parse { .. } => "browser-registry-parse-failed",
        BrowserRegistryError::Write { .. } => "browser-registry-write-failed",
        BrowserRegistryError::CreatePrivateDir { .. } => "browser-runtime-dir-failed",
        BrowserRegistryError::DuplicateAccountId(_) => "browser-account-duplicate",
        BrowserRegistryError::LeaseInvalid(_) => "browser-lease-invalid",
        BrowserRegistryError::EmptyAccountId => "browser-account-empty",
        BrowserRegistryError::InvalidAccountId(_) => "browser-account-invalid",
    }
}

//! Run lifecycle: supervisor + per-tab actors + deploy queue.
//!
pub mod tab;

pub use tab::{TabState, TabTransitionError};

use std::{collections::BTreeSet, path::PathBuf, sync::Arc, time::Duration};

use async_trait::async_trait;
use jailgun_core::{EventKind, JailgunEvent, Severity};
use jailgun_deploy::{
    cleanup_remote_checkout,
    shell::{SshCiTracker, SshRemoteGit, SshRemoteJob, SshRemoteUpload},
    CleanupRequest, DeployError, DeployOutcome, DeployReceipt, DeployRequest, JsonReceiptWriter,
};
use serde::{Deserialize, Serialize};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::sync::{broadcast, mpsc, oneshot, watch, Semaphore};

use crate::{
    bridge::{
        envelope_for_command, spawn_bridge, BridgeCommand, BridgeEvent, BridgeHandle,
        BridgeSpawnConfig, HelloPayload, MonitorTabPayload, OpenTabPayload, ProtocolError,
        ShutdownPayload, SubmitPromptPayload, UploadArchivePayload, PROTOCOL_VERSION,
    },
    config::RunOptions,
    errors::OrchestratorError,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunSummary {
    pub run_id: String,
    pub total_tabs: u16,
    pub downloaded: u16,
    pub deployed: u16,
    pub failures: Vec<(u16, String)>,
    pub denied_github_prompts: u32,
    pub allowed_info_prompts: u32,
}

pub struct OrchestratorHandle {
    pub events_rx: tokio::sync::broadcast::Receiver<jailgun_core::JailgunEvent>,
    pub completion: tokio::sync::oneshot::Receiver<RunSummary>,
    pub shutdown: tokio::sync::watch::Sender<bool>,
}

pub async fn run_orchestration(opts: RunOptions) -> Result<OrchestratorHandle, OrchestratorError> {
    if opts.bridge_cmd.is_empty() {
        return Err(OrchestratorError::Config(
            "bridge_cmd cannot be empty; pass --bridge-cmd or set JAILGUN_BRIDGE_CMD".into(),
        ));
    }
    let (events_tx, events_rx) = broadcast::channel(opts.event_buffer.max(64));
    let (completion_tx, completion_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let bridge = spawn_bridge(BridgeSpawnConfig {
        command: opts.bridge_cmd.clone(),
        env: opts.bridge_env.clone(),
    })
    .await?;

    tokio::spawn(async move {
        let summary = drive_run(opts, bridge, events_tx, shutdown_rx).await;
        let _ = completion_tx.send(summary);
    });

    Ok(OrchestratorHandle {
        events_rx,
        completion: completion_rx,
        shutdown: shutdown_tx,
    })
}

pub mod deploy_queue;
pub mod events;
pub use deploy_queue::{run_deploy_queue, DeployJob, DeployQueue};
pub use events::map_bridge_event;

async fn drive_run(
    opts: RunOptions,
    mut bridge: BridgeHandle,
    events: broadcast::Sender<JailgunEvent>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> RunSummary {
    let opts = Arc::new(opts);
    let total_tabs = opts.tabs();
    let mut summary = RunSummary {
        run_id: opts.run_id.clone(),
        total_tabs,
        downloaded: 0,
        deployed: 0,
        failures: Vec::new(),
        denied_github_prompts: 0,
        allowed_info_prompts: 0,
    };
    publish(
        &events,
        JailgunEvent::new(opts.run_id.clone(), EventKind::RunStarted, "run started")
            .with_field("tabs", total_tabs.to_string()),
    );

    if let Err(error) = send_bridge_hello(&opts, &bridge.commands_tx).await {
        summary.failures.push((0, error.to_string()));
        publish_error(&events, &opts.run_id, None, error.to_string());
        return summary;
    }

    if let Err(error) = wait_for_bridge_ready(&opts, &events, &mut bridge).await {
        summary.failures.push((0, error.clone()));
        publish_error(&events, &opts.run_id, None, error);
        let _ = tokio::time::timeout(Duration::from_secs(5), bridge.child.wait()).await;
        return summary;
    }

    let mut tracker = RunTracker::new(total_tabs, !opts.no_deploy && opts.config.deploy.enabled);
    let mut launcher = LaunchScheduler::new(total_tabs);
    let (launch_tx, mut launch_rx) = mpsc::channel::<LaunchTrigger>(total_tabs as usize + 1);
    if let Err(error) = launcher
        .launch_next(&opts, &bridge.commands_tx, &events)
        .await
    {
        summary.failures.push((0, error.to_string()));
        publish_error(&events, &opts.run_id, None, error.to_string());
        return summary;
    }

    let (deploy_result_tx, mut deploy_result_rx) =
        mpsc::channel::<DeployResult>(total_tabs as usize + 1);
    let deploy_semaphore = Arc::new(Semaphore::new(opts.deploy_concurrency.max(1) as usize));
    let deadline = tokio::time::sleep(run_deadline(&opts, total_tabs));
    tokio::pin!(deadline);

    loop {
        if run_is_complete(&tracker) {
            break;
        }
        tokio::select! {
            _ = &mut deadline => {
                summary.failures.push((0, "run timed out waiting for bridge/deploy completion".into()));
                publish_error(&events, &opts.run_id, None, "run timed out waiting for bridge/deploy completion");
                break;
            }
            changed = shutdown_rx.changed() => {
                match changed {
                    Ok(()) if *shutdown_rx.borrow() => {
                        publish_error(&events, &opts.run_id, None, "run cancelled");
                        break;
                    }
                    Ok(()) => {}
                    Err(_) => break,
                }
            }
            Some(trigger) = launch_rx.recv() => {
                if launcher.consume_scheduled_launch(trigger.tab_id) {
                    if let Err(error) = launcher.launch_next(&opts, &bridge.commands_tx, &events).await {
                        summary.failures.push((0, error.to_string()));
                        publish_error(&events, &opts.run_id, None, error.to_string());
                        break;
                    }
                }
            }
            Some(result) = deploy_result_rx.recv() => {
                match result.result {
                    Ok(()) => {
                        tracker.mark_deployed(result.tab_id);
                        summary.deployed = tracker.deployed_count();
                    }
                    Err(reason) => {
                        summary.failures.push((result.tab_id, reason.clone()));
                        publish_error(&events, &opts.run_id, Some(result.tab_id), reason);
                        tracker.mark_terminal(result.tab_id);
                    }
                }
            }
            maybe = bridge.events_rx.recv() => {
                match maybe {
                    Some(Ok(envelope)) => {
                        let effects = handle_bridge_envelope(
                        &opts,
                        &events,
                        &deploy_result_tx,
                        deploy_semaphore.clone(),
                        envelope,
                        &mut summary,
                        &mut tracker,
                        ).await;
                        if let Some(tab_id) = effects.prompt_submitted {
                            if let Some(delay) = launcher.prompt_accepted(tab_id, submit_delay(&opts)) {
                                schedule_launch_timer(
                                    &events,
                                    &opts.run_id,
                                    &launch_tx,
                                    delay.tab_id,
                                    delay.duration,
                                    delay.reason,
                                );
                            }
                        }
                        if let Some(tab_id) = effects.terminal_tab {
                            if let Some(delay) = launcher.tab_terminal(tab_id, submit_delay(&opts)) {
                                schedule_launch_timer(
                                    &events,
                                    &opts.run_id,
                                    &launch_tx,
                                    delay.tab_id,
                                    delay.duration,
                                    delay.reason,
                                );
                            }
                        }
                    }
                    Some(Err(error)) => {
                        summary.failures.push((0, error.to_string()));
                        publish_error(&events, &opts.run_id, None, error.to_string());
                    }
                    None => {
                        if !run_is_complete(&tracker) {
                            summary.failures.push((0, "bridge exited before run completed".into()));
                            publish_error(&events, &opts.run_id, None, "bridge exited before run completed");
                        }
                        break;
                    }
                }
            }
        }
    }

    let _ = send_command(
        &bridge.commands_tx,
        &opts.run_id,
        None,
        BridgeCommand::Shutdown(ShutdownPayload {
            drain_timeout_ms: 5_000,
        }),
    )
    .await;
    let _ = tokio::time::timeout(Duration::from_secs(5), bridge.child.wait()).await;
    summary
}

async fn send_bridge_hello(
    opts: &RunOptions,
    commands: &mpsc::Sender<crate::bridge::Envelope<serde_json::Value>>,
) -> Result<(), OrchestratorError> {
    send_command(
        commands,
        &opts.run_id,
        None,
        BridgeCommand::Hello(HelloPayload {
            orchestrator_version: env!("CARGO_PKG_VERSION").into(),
            protocol_version: PROTOCOL_VERSION,
            capabilities: vec![
                "source-upload".into(),
                "tar-capture".into(),
                "rust-deploy".into(),
            ],
        }),
    )
    .await?;

    Ok(())
}

async fn wait_for_bridge_ready(
    opts: &RunOptions,
    events: &broadcast::Sender<JailgunEvent>,
    bridge: &mut BridgeHandle,
) -> Result<(), String> {
    let timeout = tokio::time::sleep(Duration::from_secs(90));
    tokio::pin!(timeout);
    loop {
        tokio::select! {
            _ = &mut timeout => {
                return Err("bridge did not report ready within 90 seconds".into());
            }
            maybe = bridge.events_rx.recv() => {
                let envelope = match maybe {
                    Some(Ok(envelope)) => envelope,
                    Some(Err(error)) => {
                        return Err(format!("bridge protocol error before ready: {}", protocol_to_string(&error)));
                    }
                    None => return Err("bridge exited before reporting ready".into()),
                };
                let tab_id = envelope.tab_id;
                let event = BridgeEvent::decode(&envelope.kind, envelope.payload)
                    .map_err(|error| format!("bridge protocol error before ready: {}", protocol_to_string(&error)))?;
                match event {
                    BridgeEvent::BridgeReady(_) => return Ok(()),
                    BridgeEvent::Error(payload) => {
                        return Err(format!("bridge startup failed: {}", payload.message));
                    }
                    other => {
                        if let Some(mapped) = map_bridge_event(&opts.run_id, tab_id, &other) {
                            publish(events, mapped);
                        }
                    }
                }
            }
        }
    }
}

async fn send_tab_commands_for_tab(
    opts: &RunOptions,
    commands: &mpsc::Sender<crate::bridge::Envelope<serde_json::Value>>,
    tab_id: u16,
) -> Result<(), OrchestratorError> {
    send_command(
        commands,
        &opts.run_id,
        Some(tab_id),
        BridgeCommand::OpenTab(OpenTabPayload {
            chat_url: opts.config.browser.chat_url.clone(),
            model: opts.config.browser.model.clone(),
            profile_dir: opts.profile_dir.display().to_string(),
        }),
    )
    .await?;
    if opts.config.source_archive.enabled {
        send_command(
            commands,
            &opts.run_id,
            Some(tab_id),
            BridgeCommand::UploadArchive(UploadArchivePayload {
                repo_url: opts.repo_url.clone(),
                ref_name: opts.config.source_archive.ref_name.clone(),
                prefix: opts.config.source_archive.prefix.clone(),
                archive_filename: opts.config.source_archive.archive_filename.clone(),
                tmp_parent: None,
                delete_after_upload: opts.config.source_archive.delete_after_upload,
                confirm_selectors: Vec::new(),
                timeout_ms: 45_000,
            }),
        )
        .await?;
    }
    send_command(
        commands,
        &opts.run_id,
        Some(tab_id),
        BridgeCommand::SubmitPrompt(SubmitPromptPayload {
            prompt: prompt_for_tab(&opts.prompt_text, tab_id, opts.tabs()),
            submit_timeout_ms: 45_000,
        }),
    )
    .await?;
    send_command(
        commands,
        &opts.run_id,
        Some(tab_id),
        BridgeCommand::MonitorTab(MonitorTabPayload {
            completion_check_ms: opts.config.browser.completion_check_seconds as u64 * 1_000,
            telemetry_tick_ms: opts.config.browser.poll_interval_seconds as u64 * 1_000,
        }),
    )
    .await?;
    Ok(())
}

async fn send_command(
    commands: &mpsc::Sender<crate::bridge::Envelope<serde_json::Value>>,
    run_id: &str,
    tab_id: Option<u16>,
    command: BridgeCommand,
) -> Result<(), OrchestratorError> {
    commands
        .send(envelope_for_command(
            &command,
            run_id,
            timestamp_now(),
            tab_id,
        ))
        .await
        .map_err(|_| OrchestratorError::BridgeExited(None))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LaunchTrigger {
    tab_id: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LaunchDelay {
    tab_id: u16,
    duration: Duration,
    reason: &'static str,
}

#[derive(Debug)]
struct LaunchScheduler {
    total_tabs: u16,
    next_tab: u16,
    waiting_for_acceptance: Option<u16>,
    scheduled_launch: Option<u16>,
}

impl LaunchScheduler {
    fn new(total_tabs: u16) -> Self {
        Self {
            total_tabs,
            next_tab: 1,
            waiting_for_acceptance: None,
            scheduled_launch: None,
        }
    }

    async fn launch_next(
        &mut self,
        opts: &RunOptions,
        commands: &mpsc::Sender<crate::bridge::Envelope<serde_json::Value>>,
        events: &broadcast::Sender<JailgunEvent>,
    ) -> Result<(), OrchestratorError> {
        if self.next_tab > self.total_tabs {
            return Ok(());
        }
        let tab_id = self.next_tab;
        self.next_tab = self.next_tab.saturating_add(1);
        self.waiting_for_acceptance = Some(tab_id);
        publish_browser_log(
            events,
            &opts.run_id,
            Some(tab_id),
            "launch-tab",
            "started",
            "opening tab and queueing upload, submit, monitor commands",
            [
                ("tab_id", tab_id.to_string()),
                ("total_tabs", self.total_tabs.to_string()),
            ],
        );
        send_tab_commands_for_tab(opts, commands, tab_id).await
    }

    fn prompt_accepted(&mut self, tab_id: u16, delay: Duration) -> Option<LaunchDelay> {
        if self.waiting_for_acceptance != Some(tab_id) {
            return None;
        }
        self.waiting_for_acceptance = None;
        self.schedule_next(delay, "prompt-accepted")
    }

    fn tab_terminal(&mut self, tab_id: u16, delay: Duration) -> Option<LaunchDelay> {
        if self.waiting_for_acceptance != Some(tab_id) {
            return None;
        }
        self.waiting_for_acceptance = None;
        self.schedule_next(delay, "tab-terminal-before-prompt-accepted")
    }

    fn schedule_next(&mut self, duration: Duration, reason: &'static str) -> Option<LaunchDelay> {
        if self.next_tab > self.total_tabs || self.scheduled_launch.is_some() {
            return None;
        }
        let tab_id = self.next_tab;
        self.scheduled_launch = Some(tab_id);
        Some(LaunchDelay {
            tab_id,
            duration,
            reason,
        })
    }

    fn consume_scheduled_launch(&mut self, tab_id: u16) -> bool {
        if self.scheduled_launch == Some(tab_id) {
            self.scheduled_launch = None;
            true
        } else {
            false
        }
    }
}

fn schedule_launch_timer(
    events: &broadcast::Sender<JailgunEvent>,
    run_id: &str,
    launch_tx: &mpsc::Sender<LaunchTrigger>,
    tab_id: u16,
    duration: Duration,
    reason: &'static str,
) {
    publish_browser_log(
        events,
        run_id,
        Some(tab_id),
        "launch-delay",
        "waiting",
        "waiting before launching next tab",
        [
            ("next_tab", tab_id.to_string()),
            ("delay_ms", duration.as_millis().to_string()),
            ("reason", reason.to_string()),
        ],
    );
    let launch_tx = launch_tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(duration).await;
        let _ = launch_tx.send(LaunchTrigger { tab_id }).await;
    });
}

#[derive(Debug)]
struct RunTracker {
    total_tabs: u16,
    deploy_required: bool,
    downloaded_tabs: BTreeSet<u16>,
    deployed_tabs: BTreeSet<u16>,
    terminal_tabs: BTreeSet<u16>,
}

impl RunTracker {
    fn new(total_tabs: u16, deploy_required: bool) -> Self {
        Self {
            total_tabs,
            deploy_required,
            downloaded_tabs: BTreeSet::new(),
            deployed_tabs: BTreeSet::new(),
            terminal_tabs: BTreeSet::new(),
        }
    }

    fn mark_downloaded(&mut self, tab_id: u16) {
        self.downloaded_tabs.insert(tab_id);
    }

    fn mark_deployed(&mut self, tab_id: u16) {
        self.deployed_tabs.insert(tab_id);
    }

    fn mark_terminal(&mut self, tab_id: u16) {
        self.terminal_tabs.insert(tab_id);
    }

    fn downloaded_count(&self) -> u16 {
        self.downloaded_tabs.len().min(u16::MAX as usize) as u16
    }

    fn deployed_count(&self) -> u16 {
        self.deployed_tabs.len().min(u16::MAX as usize) as u16
    }

    fn tab_is_complete(&self, tab_id: u16) -> bool {
        if self.terminal_tabs.contains(&tab_id) {
            return true;
        }
        if !self.downloaded_tabs.contains(&tab_id) {
            return false;
        }
        !self.deploy_required || self.deployed_tabs.contains(&tab_id)
    }

    fn is_complete(&self) -> bool {
        (1..=self.total_tabs).all(|tab_id| self.tab_is_complete(tab_id))
    }
}

#[derive(Debug, Default)]
struct BridgeEffects {
    prompt_submitted: Option<u16>,
    terminal_tab: Option<u16>,
}

async fn handle_bridge_envelope(
    opts: &Arc<RunOptions>,
    events: &broadcast::Sender<JailgunEvent>,
    deploy_result_tx: &mpsc::Sender<DeployResult>,
    deploy_semaphore: Arc<Semaphore>,
    envelope: crate::bridge::Envelope<serde_json::Value>,
    summary: &mut RunSummary,
    tracker: &mut RunTracker,
) -> BridgeEffects {
    let mut effects = BridgeEffects::default();
    let tab_id = envelope.tab_id;
    let decoded = BridgeEvent::decode(&envelope.kind, envelope.payload)
        .map_err(|error| protocol_to_string(&error));
    let event = match decoded {
        Ok(event) => event,
        Err(error) => {
            summary.failures.push((tab_id.unwrap_or(0), error.clone()));
            publish_error(events, &opts.run_id, tab_id, error);
            if let Some(tab_id) = tab_id {
                tracker.mark_terminal(tab_id);
                effects.terminal_tab = Some(tab_id);
            }
            return effects;
        }
    };

    if let Some(mapped) = map_bridge_event(&opts.run_id, tab_id, &event) {
        publish(events, mapped);
    }

    match event {
        BridgeEvent::PromptSubmitted(_) => {
            if let Some(tab_id) = tab_id {
                effects.prompt_submitted = Some(tab_id);
            }
        }
        BridgeEvent::DownloadComplete(payload) => {
            if let Some(tab_id) = tab_id {
                if let Err(reason) = validate_download_archive(opts, tab_id, &payload.local_path) {
                    summary.failures.push((tab_id, reason.clone()));
                    publish_error(events, &opts.run_id, Some(tab_id), reason);
                    tracker.mark_terminal(tab_id);
                    effects.terminal_tab = Some(tab_id);
                    return effects;
                }
                tracker.mark_downloaded(tab_id);
                summary.downloaded = tracker.downloaded_count();
                if opts.no_deploy || !opts.config.deploy.enabled {
                    tracker.mark_deployed(tab_id);
                    summary.deployed = tracker.deployed_count();
                } else {
                    publish(
                        events,
                        JailgunEvent::new(
                            opts.run_id.clone(),
                            EventKind::DeployQueued,
                            "deploy queued",
                        )
                        .with_tab(tab_id)
                        .with_field("phase", "deploy-queue")
                        .with_field("status", "queued")
                        .with_field("local_path", payload.local_path.clone())
                        .with_field("sha256", payload.sha256.clone()),
                    );
                    let opts = Arc::clone(opts);
                    let events = events.clone();
                    let deploy_result_tx = deploy_result_tx.clone();
                    tokio::spawn(async move {
                        let permit = deploy_semaphore.acquire_owned().await;
                        let result = match permit {
                            Ok(_permit) => {
                                publish(
                                    &events,
                                    JailgunEvent::new(
                                        opts.run_id.clone(),
                                        EventKind::DeployQueued,
                                        "deploy started",
                                    )
                                    .with_tab(tab_id)
                                    .with_field("phase", "deploy-queue")
                                    .with_field("status", "started"),
                                );
                                deploy_download(
                                    &opts,
                                    &events,
                                    tab_id,
                                    payload.local_path,
                                    payload.local_name,
                                )
                                .await
                            }
                            Err(_) => Err("deploy semaphore closed".into()),
                        };
                        let _ = deploy_result_tx.send(DeployResult { tab_id, result }).await;
                    });
                }
            }
        }
        BridgeEvent::PromptPolicyApplied(payload) => match payload.decision.as_str() {
            "deny" | "denied" => {
                summary.denied_github_prompts = summary.denied_github_prompts.saturating_add(1)
            }
            "allow-info" | "allowed-info" | "allow" => {
                summary.allowed_info_prompts = summary.allowed_info_prompts.saturating_add(1)
            }
            _ => {}
        },
        BridgeEvent::Error(payload) => {
            summary
                .failures
                .push((tab_id.unwrap_or(0), payload.message.clone()));
            if let Some(tab_id) = tab_id {
                if !payload.recoverable {
                    tracker.mark_terminal(tab_id);
                    effects.terminal_tab = Some(tab_id);
                }
            }
        }
        BridgeEvent::BridgeShuttingDown(_) if !run_is_complete(tracker) => {
            summary
                .failures
                .push((0, "bridge shut down before run completed".into()));
        }
        _ => {}
    }
    effects
}

async fn deploy_download(
    opts: &RunOptions,
    events: &broadcast::Sender<JailgunEvent>,
    tab_id: u16,
    local_path: String,
    local_name: String,
) -> Result<(), String> {
    let archive_path = PathBuf::from(&local_path);
    let require_single_top_level = opts.config.deploy.remote_strip_components > 0;
    let validation = jailgun_core::validate_tar_gz(&archive_path, require_single_top_level)
        .map_err(|error| error.to_string())?;
    if let Some(expected) = opts.deploy_expected_top_level.as_deref() {
        if validation.top_level.as_deref() != Some(expected) {
            return Err(format!(
                "archive top-level must be {expected}/, found {}; refusing remote upload",
                validation.top_level.as_deref().unwrap_or("(multiple)")
            ));
        }
    }

    let remote_host = opts
        .deploy_remote_host
        .clone()
        .ok_or_else(|| "deploy remote host is not configured".to_string())?;
    let remote_dir = opts
        .deploy_remote_dir
        .clone()
        .ok_or_else(|| "deploy remote dir is not configured".to_string())?;
    let remote_command = opts
        .deploy_remote_command
        .clone()
        .unwrap_or_else(|| "bash ci-fast-push.sh".into());
    let receipt_dir = opts.artifacts_dir.join("receipts").join(&opts.run_id);
    let archive_name = if local_name.trim().is_empty() {
        archive_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("chatgpt-output.tar.gz")
            .to_string()
    } else {
        local_name
    };

    let mut git = SshRemoteGit::new(remote_host.clone(), receipt_dir.clone());
    cleanup_remote_checkout(
        &mut git,
        CleanupRequest {
            run_id: opts.run_id.clone(),
            tab_id: Some(tab_id),
            remote_host: remote_host.clone(),
            remote_dir: remote_dir.clone(),
            policy: opts.config.deploy.remote_cleanup_policy,
            receipt_dir: receipt_dir.clone(),
        },
    )
    .await
    .map_err(|error| error.to_string())?;

    let mut upload = SshRemoteUpload::new(remote_host.clone());
    let mut job = SshRemoteJob::new(remote_host.clone());
    let mut ci = SshCiTracker::with_repo(opts.ci_repo.clone());
    let mut writer = LocalReceiptWriter {
        receipt_dir: receipt_dir.clone(),
    };
    let receipt = jailgun_deploy::deploy_remote(
        &mut upload,
        &mut job,
        &mut ci,
        &mut writer,
        DeployRequest {
            run_id: opts.run_id.clone(),
            tab_id,
            remote_host,
            remote_dir,
            remote_command,
            remote_archive_basename: archive_name,
            local_archive_path: archive_path,
            strip_components: opts.config.deploy.remote_strip_components,
            cleanup_policy: opts.config.deploy.remote_cleanup_policy,
            receipt_dir,
            status_poll_seconds: opts.config.deploy.remote_status_poll_seconds,
            status_max_minutes: opts.status_max_minutes,
            ci_tracker_enabled: opts.ci_tracker_enabled,
            ci_repo: opts.ci_repo.clone(),
            ci_branch: opts.ci_branch.clone(),
            ci_max_attempts: opts.ci_max_attempts,
            ci_poll_seconds: opts.ci_poll_seconds,
            stash_on_failure: true,
            dry_run: opts.dry_run || opts.config.deploy.dry_run,
        },
        events,
    )
    .await
    .map_err(|error| error.to_string())?;

    if deploy_outcome_succeeded(receipt.outcome) {
        Ok(())
    } else {
        let mut reason = format!(
            "deploy finished with outcome {}",
            deploy_outcome_label(receipt.outcome)
        );
        if let Some(failure_reason) = receipt.final_status.failure_reason.as_deref() {
            reason.push_str(&format!("; failure_reason={failure_reason}"));
        }
        if let Some(exit_code) = receipt.final_status.exit_code {
            reason.push_str(&format!("; exit_code={exit_code}"));
        }
        if let Some(line) = receipt
            .log_tail
            .lines()
            .find(|line| !line.trim().is_empty())
        {
            reason.push_str(&format!("; log_tail={}", line.trim()));
        }
        Err(reason)
    }
}

fn deploy_outcome_succeeded(outcome: DeployOutcome) -> bool {
    matches!(
        outcome,
        DeployOutcome::Succeeded | DeployOutcome::SucceededCiSkipped | DeployOutcome::DryRunStaged
    )
}

fn deploy_outcome_label(outcome: DeployOutcome) -> &'static str {
    match outcome {
        DeployOutcome::Succeeded => "succeeded",
        DeployOutcome::SucceededCiFailed => "succeeded-ci-failed",
        DeployOutcome::SucceededCiSkipped => "succeeded-ci-skipped",
        DeployOutcome::FailedPreserved => "failed-preserved",
        DeployOutcome::FailedHard => "failed-hard",
        DeployOutcome::UploadShaMismatch => "upload-sha-mismatch",
        DeployOutcome::TimedOut => "timed-out",
        DeployOutcome::DryRunStaged => "dry-run-staged",
    }
}

fn validate_download_archive(
    opts: &RunOptions,
    tab_id: u16,
    local_path: &str,
) -> Result<(), String> {
    let archive_path = PathBuf::from(local_path);
    let require_single_top_level =
        opts.config.deploy.remote_strip_components > 0 || opts.deploy_expected_top_level.is_some();
    let validation = jailgun_core::validate_tar_gz(&archive_path, require_single_top_level)
        .map_err(|error| error.to_string())?;
    if validation.entry_count == 0 {
        return Err(format!(
            "tab {tab_id} downloaded archive has zero tar entries: {}",
            archive_path.display()
        ));
    }
    if let Some(expected) = opts.deploy_expected_top_level.as_deref() {
        if validation.top_level.as_deref() != Some(expected) {
            return Err(format!(
                "archive top-level must be {expected}/, found {}; refusing remote upload",
                validation.top_level.as_deref().unwrap_or("(multiple)")
            ));
        }
    }
    Ok(())
}

fn run_is_complete(tracker: &RunTracker) -> bool {
    tracker.is_complete()
}

fn run_deadline(opts: &RunOptions, total_tabs: u16) -> Duration {
    let tar_wait_seconds = opts.config.browser.tar_wait_minutes.max(1) as u64 * 60;
    let stagger_seconds = (opts.config.browser.submit_delay_seconds as u64
        + opts.config.browser.submit_jitter_seconds as u64)
        * total_tabs.saturating_sub(1) as u64;
    Duration::from_secs(tar_wait_seconds + stagger_seconds + 60)
}

fn submit_delay(opts: &RunOptions) -> Duration {
    let base_ms = opts.config.browser.submit_delay_seconds as u64 * 1_000;
    let jitter_ms = opts.config.browser.submit_jitter_seconds as u64 * 1_000;
    let jitter = if jitter_ms == 0 {
        0
    } else {
        let nanos = OffsetDateTime::now_utc().unix_timestamp_nanos() as u128;
        (nanos % (jitter_ms as u128 + 1)) as u64
    };
    Duration::from_millis(base_ms + jitter)
}

fn prompt_for_tab(prompt: &str, tab_id: u16, total_tabs: u16) -> String {
    let with_placeholders = prompt
        .replace("{{TAB_INDEX}}", &tab_id.to_string())
        .replace("{{TAB_NUMBER}}", &tab_id.to_string())
        .replace("{{TAB_COUNT}}", &total_tabs.to_string());
    format!(
        "Batch tab: {tab_id} of {total_tabs}.\nUse this tab number ({tab_id}) in your final response and artifact notes.\n\n{with_placeholders}"
    )
}

fn publish(events: &broadcast::Sender<JailgunEvent>, event: JailgunEvent) {
    let _ = events.send(event);
}

fn publish_browser_log<I, K, V>(
    events: &broadcast::Sender<JailgunEvent>,
    run_id: &str,
    tab_id: Option<u16>,
    phase: &str,
    status: &str,
    message: &str,
    fields: I,
) where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
{
    let mut event = JailgunEvent::new(run_id.to_string(), EventKind::BrowserLog, message)
        .with_field("phase", phase.to_string())
        .with_field("status", status.to_string());
    if let Some(tab_id) = tab_id {
        event = event.with_tab(tab_id);
    }
    for (key, value) in fields {
        event = event.with_field(key, value);
    }
    publish(events, event);
}

fn publish_error(
    events: &broadcast::Sender<JailgunEvent>,
    run_id: &str,
    tab_id: Option<u16>,
    error: impl Into<String>,
) {
    let mut event = JailgunEvent::new(run_id.to_string(), EventKind::Error, error.into())
        .with_severity(Severity::Error);
    if let Some(tab_id) = tab_id {
        event = event.with_tab(tab_id);
    }
    publish(events, event);
}

fn protocol_to_string(error: &ProtocolError) -> String {
    error.to_string()
}

fn timestamp_now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

struct DeployResult {
    tab_id: u16,
    result: Result<(), String>,
}

struct LocalReceiptWriter {
    receipt_dir: PathBuf,
}

#[async_trait]
impl JsonReceiptWriter for LocalReceiptWriter {
    async fn write_receipt(&mut self, receipt: &DeployReceipt) -> Result<PathBuf, DeployError> {
        tokio::fs::create_dir_all(&self.receipt_dir).await?;
        let path = self.receipt_dir.join(format!(
            "{}-tab-{:02}-deploy.json",
            receipt.run_id, receipt.tab_id
        ));
        let bytes = serde_json::to_vec_pretty(receipt)?;
        tokio::fs::write(&path, bytes).await?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_for_tab_prefixes_and_replaces_placeholders() {
        let prompt = "Build {{TAB_INDEX}} of {{TAB_COUNT}}.";
        let got = prompt_for_tab(prompt, 2, 7);
        assert!(got.starts_with("Batch tab: 2 of 7."));
        assert!(got.contains("Build 2 of 7."));
    }

    #[test]
    fn launch_scheduler_waits_for_prompt_acceptance_before_next_tab() {
        let mut scheduler = LaunchScheduler::new(3);
        assert_eq!(scheduler.next_tab, 1);
        scheduler.next_tab = 2;
        scheduler.waiting_for_acceptance = Some(1);

        assert!(scheduler
            .prompt_accepted(2, Duration::from_secs(60))
            .is_none());
        let delay = scheduler
            .prompt_accepted(1, Duration::from_secs(60))
            .expect("tab 2 scheduled after tab 1 acceptance");
        assert_eq!(delay.tab_id, 2);
        assert_eq!(delay.duration, Duration::from_secs(60));
        assert!(scheduler.consume_scheduled_launch(2));
    }

    #[test]
    fn run_tracker_completes_when_a_tab_fails_before_download() {
        let mut tracker = RunTracker::new(2, true);
        tracker.mark_terminal(1);
        assert!(!tracker.is_complete());
        tracker.mark_downloaded(2);
        assert!(!tracker.is_complete());
        tracker.mark_deployed(2);
        assert!(tracker.is_complete());
    }

    #[test]
    fn failed_preserved_deploy_outcome_is_not_successful() {
        assert!(deploy_outcome_succeeded(DeployOutcome::Succeeded));
        assert!(deploy_outcome_succeeded(DeployOutcome::SucceededCiSkipped));
        assert!(!deploy_outcome_succeeded(DeployOutcome::FailedPreserved));
        assert!(!deploy_outcome_succeeded(DeployOutcome::SucceededCiFailed));
    }
}

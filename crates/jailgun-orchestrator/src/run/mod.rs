//! Run lifecycle: supervisor + per-tab actors + deploy queue.
//!
pub mod tab;

pub use tab::{TabState, TabTransitionError};

use std::{path::PathBuf, sync::Arc, time::Duration};

use async_trait::async_trait;
use jailgun_core::{EventKind, JailgunEvent, Severity};
use jailgun_deploy::{
    cleanup_remote_checkout,
    shell::{SshCiTracker, SshRemoteGit, SshRemoteJob, SshRemoteUpload},
    CleanupRequest, DeployError, DeployReceipt, DeployRequest, JsonReceiptWriter,
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

    if let Err(error) = send_initial_commands(&opts, &bridge.commands_tx).await {
        summary.failures.push((0, error.to_string()));
        publish_error(&events, &opts.run_id, None, error.to_string());
        return summary;
    }

    let (deploy_result_tx, mut deploy_result_rx) =
        mpsc::channel::<DeployResult>(total_tabs as usize + 1);
    let deploy_semaphore = Arc::new(Semaphore::new(opts.deploy_concurrency.max(1) as usize));
    let deadline = tokio::time::sleep(Duration::from_secs(
        opts.config.browser.tar_wait_minutes.max(1) as u64 * 60,
    ));
    tokio::pin!(deadline);

    loop {
        if run_is_complete(&opts, &summary) {
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
            Some(result) = deploy_result_rx.recv() => {
                match result.result {
                    Ok(()) => summary.deployed = summary.deployed.saturating_add(1),
                    Err(reason) => {
                        summary.failures.push((result.tab_id, reason.clone()));
                        publish_error(&events, &opts.run_id, Some(result.tab_id), reason);
                    }
                }
            }
            maybe = bridge.events_rx.recv() => {
                match maybe {
                    Some(Ok(envelope)) => handle_bridge_envelope(
                        &opts,
                        &events,
                        &deploy_result_tx,
                        deploy_semaphore.clone(),
                        envelope,
                        &mut summary,
                    ).await,
                    Some(Err(error)) => {
                        summary.failures.push((0, error.to_string()));
                        publish_error(&events, &opts.run_id, None, error.to_string());
                    }
                    None => break,
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

async fn send_initial_commands(
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

    for tab_id in 1..=opts.tabs() {
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
                prompt: opts.prompt_text.clone(),
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
    }
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

async fn handle_bridge_envelope(
    opts: &Arc<RunOptions>,
    events: &broadcast::Sender<JailgunEvent>,
    deploy_result_tx: &mpsc::Sender<DeployResult>,
    deploy_semaphore: Arc<Semaphore>,
    envelope: crate::bridge::Envelope<serde_json::Value>,
    summary: &mut RunSummary,
) {
    let tab_id = envelope.tab_id;
    let decoded = BridgeEvent::decode(&envelope.kind, envelope.payload)
        .map_err(|error| protocol_to_string(&error));
    let event = match decoded {
        Ok(event) => event,
        Err(error) => {
            summary.failures.push((tab_id.unwrap_or(0), error.clone()));
            publish_error(events, &opts.run_id, tab_id, error);
            return;
        }
    };

    if let Some(mapped) = map_bridge_event(&opts.run_id, tab_id, &event) {
        publish(events, mapped);
    }

    match event {
        BridgeEvent::DownloadComplete(payload) => {
            if let Some(tab_id) = tab_id {
                summary.downloaded = summary.downloaded.saturating_add(1);
                if opts.no_deploy || !opts.config.deploy.enabled {
                    summary.deployed = summary.deployed.saturating_add(1);
                } else {
                    let opts = Arc::clone(opts);
                    let events = events.clone();
                    let deploy_result_tx = deploy_result_tx.clone();
                    tokio::spawn(async move {
                        let permit = deploy_semaphore.acquire_owned().await;
                        let result = match permit {
                            Ok(_permit) => {
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
        }
        BridgeEvent::BridgeShuttingDown(_) if !run_is_complete(opts, summary) => {
            summary
                .failures
                .push((0, "bridge shut down before run completed".into()));
        }
        _ => {}
    }
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
    let mut ci = SshCiTracker::new();
    let mut writer = LocalReceiptWriter {
        receipt_dir: receipt_dir.clone(),
    };
    jailgun_deploy::deploy_remote(
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
            ci_branch: opts.ci_branch.clone(),
            ci_max_attempts: opts.ci_max_attempts,
            ci_poll_seconds: opts.ci_poll_seconds,
            stash_on_failure: true,
            dry_run: opts.dry_run || opts.config.deploy.dry_run,
        },
        events,
    )
    .await
    .map(|_| ())
    .map_err(|error| error.to_string())
}

fn run_is_complete(opts: &RunOptions, summary: &RunSummary) -> bool {
    summary.downloaded >= opts.tabs()
        && (opts.no_deploy || !opts.config.deploy.enabled || summary.deployed >= opts.tabs())
}

fn publish(events: &broadcast::Sender<JailgunEvent>, event: JailgunEvent) {
    let _ = events.send(event);
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

use std::sync::Arc;

use jailgun_core::{EventKind, JailgunEvent};
use tokio::sync::{broadcast, mpsc, Semaphore};

use crate::{
    bridge::BridgeEvent,
    config::RunOptions,
    run::{
        bridge_flow::protocol_to_string,
        deploy::{deploy_download, validate_download_archive},
        events::map_bridge_event,
        publish::{publish, publish_error},
        tracker::{run_is_complete, RunTracker},
        RunSummary,
    },
};

#[derive(Debug, Default)]
pub(super) struct BridgeEffects {
    pub(super) prompt_submitted: Option<u16>,
    pub(super) terminal_tab: Option<u16>,
    pub(super) failed_tab: Option<u16>,
}

pub(super) async fn handle_bridge_envelope(
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
                let is_archive = download_complete_is_archive(&payload);
                if is_archive {
                    if let Err(reason) =
                        validate_download_archive(opts, tab_id, &payload.local_path)
                    {
                        summary.failures.push((tab_id, reason.clone()));
                        publish_error(events, &opts.run_id, Some(tab_id), reason);
                        tracker.mark_terminal(tab_id);
                        effects.terminal_tab = Some(tab_id);
                        return effects;
                    }
                } else if !opts.no_deploy && opts.config.deploy.enabled {
                    let reason =
                        "non-archive downloads cannot be deployed; expected a .tar.gz artifact"
                            .to_string();
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
                    if tracker.tab_session_expired(tab_id) {
                        effects.failed_tab = Some(tab_id);
                    } else {
                        effects.terminal_tab = Some(tab_id);
                    }
                }
            }
        }
        BridgeEvent::SessionExpired(payload) => {
            summary
                .failures
                .push((tab_id.unwrap_or(0), payload.reason.clone()));
            if let Some(tab_id) = tab_id {
                tracker.mark_session_expired(tab_id);
                tracker.mark_terminal(tab_id);
                effects.failed_tab = Some(tab_id);
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

fn download_complete_is_archive(payload: &crate::bridge::DownloadCompletePayload) -> bool {
    match payload.file_kind.as_deref() {
        Some("downloaded-archive") | Some("archive") | Some("tar-gz") => true,
        Some("downloaded-tex") | Some("downloaded-file") | Some("tex") | Some("file") => false,
        Some(_) | None => payload.local_path.to_ascii_lowercase().ends_with(".tar.gz"),
    }
}

pub(super) struct DeployResult {
    pub(super) tab_id: u16,
    pub(super) result: Result<(), String>,
}

//! Run lifecycle: supervisor + per-tab actors + deploy queue.
//!
mod bridge_events;
mod bridge_flow;
mod deploy;
pub mod deploy_queue;
pub mod events;
mod launch;
mod publish;
pub mod tab;
mod timing;
mod tracker;

pub use deploy_queue::{run_deploy_queue, DeployJob, DeployQueue};
pub use events::map_bridge_event;
pub use tab::{TabState, TabTransitionError};

use std::{sync::Arc, time::Duration};

use jailgun_core::{EventKind, JailgunEvent};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, oneshot, watch, Semaphore};

use crate::{
    bridge::{spawn_bridge, BridgeCommand, BridgeHandle, BridgeSpawnConfig, ShutdownPayload},
    config::RunOptions,
    errors::OrchestratorError,
    run::{
        bridge_events::{handle_bridge_envelope, DeployResult},
        bridge_flow::{send_bridge_hello, send_command, wait_for_bridge_ready},
        launch::{schedule_launch_timer, LaunchScheduler, LaunchTrigger},
        publish::{publish, publish_error},
        timing::{run_deadline, submit_delay},
        tracker::{run_is_complete, RunTracker},
    },
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
    jailgun_core::validate_run_id(&opts.run_id).map_err(OrchestratorError::Config)?;
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
                        if let Some(tab_id) = effects.failed_tab {
                            launcher.tab_failed(tab_id);
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

#[cfg(test)]
mod tests;

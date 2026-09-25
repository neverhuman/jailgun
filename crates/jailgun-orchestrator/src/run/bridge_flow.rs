use std::time::Duration;

use jailgun_core::JailgunEvent;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::sync::{broadcast, mpsc};

use crate::{
    bridge::{
        envelope_for_command, BridgeCommand, BridgeEvent, BridgeHandle, HelloPayload,
        MonitorTabPayload, OpenTabPayload, ProtocolError, SubmitPromptPayload,
        UploadArchivePayload, PROTOCOL_VERSION,
    },
    config::RunOptions,
    errors::OrchestratorError,
    run::{events::map_bridge_event, publish::publish},
};

pub(super) async fn send_bridge_hello(
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

pub(super) async fn wait_for_bridge_ready(
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

pub(super) async fn send_tab_commands_for_tab(
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
            profile_dir: open_tab_profile_dir(opts, tab_id),
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
                local_archive_path: opts
                    .local_archive_path
                    .as_ref()
                    .map(|path| path.display().to_string()),
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

pub(super) async fn send_command(
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

pub(super) fn open_tab_profile_dir(opts: &RunOptions, tab_id: u16) -> Option<String> {
    if let Some(path) = opts.tab_profile_dirs.get(&tab_id) {
        return Some(path.display().to_string());
    }
    if opts.profile_pool.len() > 1 {
        None
    } else {
        Some(opts.profile_dir.display().to_string())
    }
}

pub(super) fn prompt_for_tab(prompt: &str, tab_id: u16, total_tabs: u16) -> String {
    let with_placeholders = prompt
        .replace("{{TAB_INDEX}}", &tab_id.to_string())
        .replace("{{TAB_NUMBER}}", &tab_id.to_string())
        .replace("{{TAB_COUNT}}", &total_tabs.to_string());
    format!(
        "Batch tab: {tab_id} of {total_tabs}.\nIf a tab identifier is relevant, include tab {tab_id} inside the requested artifact or notes only. Do not answer with the tab number by itself.\n\n{with_placeholders}"
    )
}

pub(super) fn protocol_to_string(error: &ProtocolError) -> String {
    error.to_string()
}

fn timestamp_now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

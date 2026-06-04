use std::{fs, path::Path};

use anyhow::{Context, Result};
use jailgun_core::{JailgunAgentRunSummary, JailgunEvent};
use tokio::io::AsyncWriteExt;

use crate::{
    agent::{
        execute_summary::build_agent_summary, timestamp_now, AgentRunBackend, AgentRunEventSink,
        PreparedAgentRun,
    },
    run::{OrchestratorHandle, RunSummary},
    support::ensure_parent_dir,
};

pub async fn execute_prepared_agent_run(
    prepared: PreparedAgentRun,
    backend: &dyn AgentRunBackend,
    sink: &dyn AgentRunEventSink,
) -> Result<JailgunAgentRunSummary> {
    let handle = backend.start(prepared.opts.clone()).await?;
    let collection = collect_agent_run_events(
        handle,
        &prepared.output_paths.events_jsonl,
        &prepared.opts.run_id,
        prepared.tabs,
        prepared.max_runtime_seconds,
        sink,
    )
    .await?;
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
}

async fn collect_agent_run_events(
    mut handle: OrchestratorHandle,
    events_jsonl: &Path,
    run_id: &str,
    tabs: u16,
    max_runtime_seconds: u64,
    sink: &dyn AgentRunEventSink,
) -> Result<AgentRunCollection> {
    ensure_parent_dir(events_jsonl)?;
    let mut file = tokio::fs::File::create(events_jsonl)
        .await
        .with_context(|| format!("creating {}", events_jsonl.display()))?;
    let mut events = Vec::new();
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
                });
            }
            event = handle.events_rx.recv(), if events_open => {
                match event {
                    Ok(event) => {
                        file.write_all(&serde_json::to_vec(&event)?).await?;
                        file.write_all(b"\n").await?;
                        sink.on_event(&event).await?;
                        events.push(event);
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
                });
            }
        }
    }
}

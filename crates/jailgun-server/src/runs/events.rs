use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use jailgun_core::{
    DeployQueueState, EventKind, JailgunAgentRunSummary, JailgunEvent, RunSnapshot, TabSnapshot,
};
use jailgun_orchestrator::{AgentRunEventSink, PreparedAgentRun};

use crate::state::AppState;

#[derive(Clone)]
pub(super) struct ServerAgentEventSink {
    pub(super) state: Arc<AppState>,
}

#[async_trait::async_trait]
impl AgentRunEventSink for ServerAgentEventSink {
    async fn on_event(&self, event: &JailgunEvent) -> anyhow::Result<()> {
        record_event(&self.state, event.clone()).await;
        if let Some(tx) = self.state.event_bus.as_ref() {
            let _ = tx.send(event.clone());
        }
        Ok(())
    }

    async fn on_summary(&self, summary: &JailgunAgentRunSummary) -> anyhow::Result<()> {
        self.state
            .agent_summaries
            .write()
            .await
            .insert(summary.run_id.clone(), summary.clone());
        let mut runs = self.state.runs.write().await;
        if let Some(run) = runs.iter_mut().find(|run| run.run_id == summary.run_id) {
            run.finished_at = Some(summary.finished_at.clone());
            run.status = summary.status.clone();
            run.denied_github_prompts = summary.denied_github_prompts;
            run.allowed_info_prompts = summary.allowed_info_prompts;
        }
        Ok(())
    }
}

pub(super) fn prepared_snapshot(prepared: &PreparedAgentRun) -> RunSnapshot {
    let status = if prepared.browser_lease.is_some() {
        "queued"
    } else {
        "running"
    };
    RunSnapshot {
        run_id: prepared.opts.run_id.clone(),
        started_at: prepared.started_at.clone(),
        finished_at: None,
        status: status.into(),
        tabs: Vec::new(),
        deploy_queue: DeployQueueState::Idle,
        denied_github_prompts: 0,
        allowed_info_prompts: 0,
    }
}

pub(super) async fn insert_run_snapshot(state: &Arc<AppState>, snapshot: RunSnapshot) {
    let mut runs = state.runs.write().await;
    if let Some(existing) = runs.iter_mut().find(|run| run.run_id == snapshot.run_id) {
        *existing = snapshot;
    } else {
        runs.insert(0, snapshot);
    }
}

pub(super) async fn mark_agent_run_failed(state: &Arc<AppState>, run_id: &str, reason: String) {
    let event = JailgunEvent::new(run_id.to_string(), EventKind::Error, reason.clone())
        .with_severity(jailgun_core::Severity::Error);
    record_event(state, event.clone()).await;
    if let Some(tx) = state.event_bus.as_ref() {
        let _ = tx.send(event.clone());
    }

    let mut runs = state.runs.write().await;
    if let Some(run) = runs.iter_mut().find(|run| run.run_id == run_id) {
        run.finished_at = Some(event.timestamp.clone());
        run.status = "failed".into();
    }
}

pub(crate) async fn record_event(state: &Arc<AppState>, event: JailgunEvent) {
    state.events.write().await.push(event.clone());
    let mut runs = state.runs.write().await;
    if let Some(run) = runs.iter_mut().find(|run| run.run_id == event.run_id) {
        apply_event_to_run(run, &event);
    } else {
        let mut run = RunSnapshot {
            run_id: event.run_id.clone(),
            started_at: event.timestamp.clone(),
            finished_at: None,
            status: "running".into(),
            tabs: Vec::new(),
            deploy_queue: DeployQueueState::Idle,
            denied_github_prompts: 0,
            allowed_info_prompts: 0,
        };
        apply_event_to_run(&mut run, &event);
        runs.insert(0, run);
    }
}

fn apply_event_to_run(run: &mut RunSnapshot, event: &JailgunEvent) {
    match event.kind {
        EventKind::RunQueued => {
            run.status = "queued".into();
        }
        EventKind::BrowserLeaseAcquired => {
            run.status = "starting".into();
        }
        EventKind::RunStarted => {
            run.started_at = event.timestamp.clone();
            run.status = event
                .fields
                .get("status")
                .cloned()
                .unwrap_or_else(|| "running".into());
        }
        EventKind::DeployQueued => {
            run.deploy_queue = DeployQueueState::Waiting;
        }
        EventKind::DeployFinished => {
            run.deploy_queue = DeployQueueState::Done;
            run.finished_at = Some(event.timestamp.clone());
        }
        EventKind::Error => {
            run.status = "failed".into();
            run.finished_at = Some(event.timestamp.clone());
        }
        EventKind::PromptPolicy => {
            bump_policy_counts(run, event);
        }
        _ => {}
    }

    if let Some(tab_id) = event.tab_id {
        let tab = upsert_tab(run, tab_id);
        apply_tab_event(tab, event);
    }
}

fn upsert_tab(run: &mut RunSnapshot, tab_id: u16) -> &mut TabSnapshot {
    if let Some(index) = run.tabs.iter().position(|tab| tab.tab_id == tab_id) {
        return &mut run.tabs[index];
    }
    run.tabs.push(TabSnapshot {
        tab_id,
        status: "active".into(),
        page_url: String::new(),
        archive_sha256: None,
        download_latency_ms: None,
        deploy_status: "pending".into(),
        prompt_policy_decision: None,
    });
    let len = run.tabs.len();
    &mut run.tabs[len - 1]
}

fn apply_tab_event(tab: &mut TabSnapshot, event: &JailgunEvent) {
    if let Some(status) = event.fields.get("tab_status") {
        tab.status = status.clone();
    }
    if let Some(page_url) = event.fields.get("page_url") {
        tab.page_url = page_url.clone();
    }
    if let Some(sha256) = event.fields.get("sha256") {
        tab.archive_sha256 = Some(sha256.clone());
    }
    if let Some(download_latency_ms) = event.fields.get("download_latency_ms") {
        tab.download_latency_ms = download_latency_ms.parse::<u64>().ok();
    }
    if let Some(deploy_status) = event.fields.get("deploy_status") {
        tab.deploy_status = deploy_status.clone();
    }
    if let Some(decision) = event.fields.get("decision") {
        tab.prompt_policy_decision = Some(decision.clone());
    }

    match event.kind {
        EventKind::TabOpened if tab.status == "active" => {
            tab.status = "opening".into();
        }
        EventKind::TabOpened => {}
        EventKind::PromptSubmitted => {
            tab.status = "submitted".into();
        }
        EventKind::TarDiscovered => {
            tab.status = "tar-discovered".into();
        }
        EventKind::DownloadReceipt => {
            tab.status = "downloaded".into();
        }
        EventKind::DeployFinished => {
            tab.deploy_status = event
                .fields
                .get("outcome")
                .cloned()
                .unwrap_or_else(|| "succeeded".into());
        }
        EventKind::RemoteSafety => {
            if let Some(policy) = event.fields.get("policy") {
                tab.deploy_status = policy.clone();
            }
        }
        EventKind::Error => {
            tab.status = "error".into();
        }
        _ => {}
    }
}

fn bump_policy_counts(run: &mut RunSnapshot, event: &JailgunEvent) {
    match event.fields.get("decision").map(String::as_str) {
        Some("deny") => run.denied_github_prompts += 1,
        Some("allow-info") => run.allowed_info_prompts += 1,
        _ => {}
    }
}

pub(super) fn agent_events_path(run_dir: &Path) -> PathBuf {
    run_dir.join("agent-events.jsonl")
}

pub(super) fn agent_summary_path(run_dir: &Path, _run_id: &str) -> PathBuf {
    run_dir.join("agent-summary.json")
}

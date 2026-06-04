use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::{
    body::Body,
    http::{HeaderMap, Request, StatusCode},
    Router,
};
use jailgun_core::{
    BrowserAccountStatus, BrowserProfileRegistry, EventKind, JailgunAgentRunRequest, JailgunConfig,
    JailgunEvent, RunSnapshot,
};
use serde_json::{json, Value};
use tokio::sync::{broadcast, mpsc};
use tower::ServiceExt;

use crate::{
    api_router,
    ws::{websocket_unauthorized, WsAuthQuery},
    AppState, BrowserAuthSession, JailgunAgentRunAcceptedResponse,
};

struct FakeBackend;

#[async_trait::async_trait]
impl jailgun_orchestrator::AgentRunBackend for FakeBackend {
    async fn start(
        &self,
        opts: jailgun_orchestrator::config::RunOptions,
    ) -> anyhow::Result<jailgun_orchestrator::OrchestratorHandle> {
        let (events_tx, events_rx) = broadcast::channel(8);
        let (completion_tx, completion_rx) = tokio::sync::oneshot::channel();
        let (shutdown_tx, _shutdown_rx) = tokio::sync::watch::channel(false);
        let run_id = opts.run_id.clone();
        tokio::spawn(async move {
            let event = JailgunEvent::new(run_id.clone(), EventKind::RunStarted, "fake started");
            let _ = events_tx.send(event);
            let summary = jailgun_orchestrator::RunSummary {
                run_id,
                total_tabs: 1,
                downloaded: 0,
                deployed: 0,
                failures: Vec::new(),
                denied_github_prompts: 0,
                allowed_info_prompts: 0,
            };
            let _ = completion_tx.send(summary);
        });
        Ok(jailgun_orchestrator::OrchestratorHandle {
            events_rx,
            completion: completion_rx,
            shutdown: shutdown_tx,
        })
    }
}

fn write_browser_registry(root: &Path, accounts: &[(&str, BrowserAccountStatus)]) -> PathBuf {
    let registry_path = root.join("browser-profiles.json");
    let roots = jailgun_core::BrowserAccountRoots {
        profile_root: root.join("profiles"),
        state_root: root.join("state"),
        downloads_root: root.join("downloads"),
    };
    let mut registry = BrowserProfileRegistry::default();
    for (index, (id, status)) in accounts.iter().enumerate() {
        registry
            .upsert_account(
                &format!("{id}@example.invalid"),
                Some((*id).to_string()),
                &roots,
                9229 + index as u16,
                3,
            )
            .unwrap();
        registry.account_mut(id).unwrap().status = *status;
    }
    registry.save(&registry_path).unwrap();
    registry_path
}

fn live_test_app(temp: &tempfile::TempDir, registry_path: PathBuf) -> Router {
    let backend = Arc::new(FakeBackend);
    let (mut state, _rx) =
        AppState::live(JailgunConfig::default(), temp.path().join("receipts"), 64);
    state.browser_registry_path = registry_path;
    api_router(
        state
            .with_ingest_token(Some("secret".into()))
            .with_agent_backend(backend),
    )
}

fn write_prompt(root: &Path, name: &str) -> PathBuf {
    let prompt_file = root.join(name);
    std::fs::write(&prompt_file, "prompt").unwrap();
    prompt_file
}

async fn post_run(app: Router, body: Value) -> (StatusCode, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/runs")
                .header("content-type", "application/json")
                .header("x-jailgun-token", "secret")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = serde_json::from_slice(&body).unwrap_or_else(|_| json!({}));
    (status, value)
}

mod browser_routes;
mod mcp_routes;
mod run_routes;
mod status_routes;

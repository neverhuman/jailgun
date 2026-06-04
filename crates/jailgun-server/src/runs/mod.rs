mod events;
mod ingress;
mod queries;

pub(crate) use events::record_event;
pub(crate) use ingress::RunIngress;
pub(crate) use queries::{get_agent_summary, get_receipts, get_run, get_runs, post_event};

use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use jailgun_core::AgentError;
use jailgun_orchestrator::{execute_prepared_agent_run, prepare_agent_run, AgentRunPaths};
use serde_json::Value;

use crate::{
    runs::{
        events::{
            agent_events_path, agent_summary_path, insert_run_snapshot, mark_agent_run_failed,
            prepared_snapshot, ServerAgentEventSink,
        },
        ingress::parse_agent_run_request,
    },
    state::{AppState, JailgunAgentRunAcceptedResponse},
};

pub(crate) async fn start_agent_run(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    start_agent_run_inner(State(state), headers, body, RunIngress::Rest).await
}

pub(crate) async fn start_agent_run_inner(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Value,
    ingress: RunIngress,
) -> Response {
    if state.event_bus.is_none() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(agent_error(
                "agent-run-unavailable",
                "start a Jailgun agent run",
                "live mode is required to start server-side runs",
            )),
        )
            .into_response();
    }
    let Some(expected) = state.ingest_token.as_deref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(agent_error(
                "agent-run-unavailable",
                "start a Jailgun agent run",
                "x-jailgun-token is required when run ingestion is enabled",
            )),
        )
            .into_response();
    };
    let provided = headers
        .get("x-jailgun-token")
        .and_then(|value| value.to_str().ok());
    if provided != Some(expected) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(agent_error(
                "agent-run-unauthorized",
                "start a Jailgun agent run",
                "x-jailgun-token did not match the configured token",
            )),
        )
            .into_response();
    }

    let mut request = match parse_agent_run_request(body, ingress) {
        Ok(request) => request,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(agent_error(
                    "agent-run-invalid",
                    "start a Jailgun agent run",
                    error,
                )),
            )
                .into_response();
        }
    };
    if request.config_path.is_none() {
        request.config_path = state.config_path.clone();
    }
    request.browser.bridge_env.insert(
        state.config.browser.profile_registry_env.clone(),
        state.browser_registry_path.display().to_string(),
    );

    let run_id = request
        .run_id
        .clone()
        .unwrap_or_else(|| format!("run-{}", uuid::Uuid::new_v4()));
    request.run_id = Some(run_id.clone());
    if state
        .runs
        .read()
        .await
        .iter()
        .any(|run| run.run_id == run_id)
    {
        return (
            StatusCode::CONFLICT,
            Json(agent_error(
                "agent-run-conflict",
                "start a Jailgun agent run",
                "run_id already exists",
            )),
        )
            .into_response();
    }
    let run_dir = state.receipt_dir.join(&run_id);
    let output_paths = AgentRunPaths {
        events_jsonl: agent_events_path(&run_dir),
        summary_json: agent_summary_path(&run_dir, &run_id),
    };
    let accepted_paths = output_paths.clone();
    let prepared = match prepare_agent_run(request, output_paths) {
        Ok(prepared) => prepared,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(agent_error(
                    "agent-run-invalid",
                    "start a Jailgun agent run",
                    format!("{error:#}"),
                )),
            )
                .into_response();
        }
    };
    if tokio::fs::try_exists(&accepted_paths.summary_json)
        .await
        .unwrap_or(false)
        || tokio::fs::try_exists(&accepted_paths.events_jsonl)
            .await
            .unwrap_or(false)
    {
        return (
            StatusCode::CONFLICT,
            Json(agent_error(
                "agent-run-conflict",
                "start a Jailgun agent run",
                "run_id output files already exist",
            )),
        )
            .into_response();
    }

    insert_run_snapshot(&state, prepared_snapshot(&prepared)).await;

    let backend = state.agent_backend.clone();
    let sink = ServerAgentEventSink {
        state: state.clone(),
    };
    let failure_run_id = run_id.clone();
    tokio::spawn(async move {
        if let Err(error) = execute_prepared_agent_run(prepared, backend.as_ref(), &sink).await {
            mark_agent_run_failed(&sink.state, &failure_run_id, error.to_string()).await;
        }
    });

    let response_run_id = run_id.clone();
    let response = JailgunAgentRunAcceptedResponse {
        run_id,
        status: "accepted".into(),
        summary_json: accepted_paths.summary_json.display().to_string(),
        events_jsonl: accepted_paths.events_jsonl.display().to_string(),
        run_url: format!("/api/runs/{response_run_id}"),
        summary_url: format!("/api/runs/{response_run_id}/agent-summary"),
    };
    (StatusCode::ACCEPTED, Json(response)).into_response()
}

fn agent_error(code: &'static str, purpose: &'static str, reason: impl Into<String>) -> AgentError {
    AgentError::new(
        code,
        purpose,
        reason.into(),
        vec![
            "verify the live server token and request body",
            "check the configured prompt file path",
            "run the mapped rust test lane if the failure persists",
        ],
        "docs/testing.md",
        "rerun the agent request with a valid prompt file and token",
    )
}

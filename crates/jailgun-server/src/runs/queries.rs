use std::sync::Arc;

use axum::{
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use jailgun_core::{validate_run_id, JailgunAgentRunSummary, JailgunEvent};
use serde_json::json;

use crate::{
    runs::events::{agent_summary_path, record_event},
    state::AppState,
};

#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunFilter {
    kind: Option<String>,
}

pub(crate) async fn get_runs(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<crate::auth::Principal>,
    filter: Result<Query<RunFilter>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let filter = match crate::concepts::query(filter) {
        Ok(filter) => filter,
        Err(error) => return error.into_response(),
    };
    if filter.kind.as_deref() == Some("archive") && !principal.is_operator() {
        return crate::auth::error(
            StatusCode::FORBIDDEN,
            "operator-required",
            "Archive runs require the operator credential.",
        );
    }
    if filter
        .kind
        .as_deref()
        .is_some_and(|kind| !["archive", "concept"].contains(&kind))
    {
        return crate::concepts::ApiError(jailgun_workflow::Error::action(
            "invalid-request",
            "Unknown run kind.",
            "Choose archive or concept.",
        ))
        .into_response();
    }
    if filter.kind.as_deref() != Some("archive")
        && (state.workflow.is_some() || filter.kind.as_deref() == Some("concept"))
    {
        match crate::concepts::store(&state) {
            Ok(store) => {
                return match store.runs().await {
                    Ok(runs) => Json(
                        runs.into_iter()
                            .filter(|run| principal.permits_account(&run.request.account_id))
                            .collect::<Vec<_>>(),
                    )
                    .into_response(),
                    Err(error) => crate::concepts::ApiError(error).into_response(),
                };
            }
            Err(error) => return error.into_response(),
        }
    }
    Json(state.runs.read().await.clone()).into_response()
}

pub(crate) async fn get_run(
    State(state): State<Arc<AppState>>,
    AxumPath(run_id): AxumPath<String>,
) -> Response {
    if let Some(store) = state.workflow.as_ref() {
        match store.run(run_id.clone()).await {
            Ok(run) => return Json(run).into_response(),
            Err(error) if error.code() == "not-found" => (),
            Err(error) => return crate::concepts::ApiError(error).into_response(),
        }
    }
    let runs = state.runs.read().await;
    match runs.iter().find(|run| run.run_id == run_id) {
        Some(run) => Json(run).into_response(),
        None => crate::concepts::ApiError(jailgun_workflow::Error::action(
            "not-found",
            "The requested run does not exist.",
            "List available runs and use a returned run ID.",
        ))
        .into_response(),
    }
}

pub(crate) async fn get_agent_summary(
    State(state): State<Arc<AppState>>,
    AxumPath(run_id): AxumPath<String>,
) -> Response {
    if validate_run_id(&run_id).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid run_id" })),
        )
            .into_response();
    }
    if let Some(summary) = state.agent_summaries.read().await.get(&run_id).cloned() {
        return Json(summary).into_response();
    }

    let summary_path = agent_summary_path(&state.receipt_dir.join(&run_id), &run_id);
    match tokio::fs::read_to_string(&summary_path).await {
        Ok(text) => match serde_json::from_str::<JailgunAgentRunSummary>(&text) {
            Ok(summary) => {
                state
                    .agent_summaries
                    .write()
                    .await
                    .insert(run_id.clone(), summary.clone());
                Json(summary).into_response()
            }
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "summary-json-invalid", "run_id": run_id })),
            )
                .into_response(),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let runs = state.runs.read().await;
            if runs.iter().any(|run| run.run_id == run_id) {
                (
                    StatusCode::ACCEPTED,
                    Json(json!({ "run_id": run_id, "status": "running" })),
                )
                    .into_response()
            } else {
                (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": "run not found" })),
                )
                    .into_response()
            }
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "summary-json-read-failed", "run_id": run_id })),
        )
            .into_response(),
    }
}

pub(crate) async fn get_receipts(
    State(state): State<Arc<AppState>>,
    AxumPath(run_id): AxumPath<String>,
) -> Response {
    if validate_run_id(&run_id).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid run_id" })),
        )
            .into_response();
    }
    let dir = state.receipt_dir.join(&run_id);
    let mut receipts = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir(&dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            match tokio::fs::read_to_string(&path).await {
                Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(value) => receipts.push(value),
                    Err(_) => receipts.push(json!({ "path": path, "error": "invalid-json" })),
                },
                Err(_) => receipts.push(json!({ "path": path, "error": "read-failed" })),
            }
        }
    }
    Json(json!({ "run_id": run_id, "receipts": receipts })).into_response()
}

pub(crate) async fn post_event(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(event): Json<JailgunEvent>,
) -> StatusCode {
    if let Some(response) = crate::auth::unauthorized(&state, &headers) {
        return response.status();
    }
    record_event(&state, event.clone()).await;
    if let Some(tx) = state.event_bus.as_ref() {
        let _ = tx.send(event);
        StatusCode::ACCEPTED
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

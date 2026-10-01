use std::sync::Arc;

use crate::AppState;
use axum::{
    extract::{
        rejection::{JsonRejection, QueryRejection},
        Path, Query, State,
    },
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use jailgun_workflow::{
    model::{ConceptRequest, ModelSelection, ResumeRequest, RunAccepted},
    Error, Store,
};
use serde::Deserialize;

pub(crate) struct ApiError(pub Error);
impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        Self(error)
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.0.code() {
            "not-found" | "account-not-found" => StatusCode::NOT_FOUND,
            "account-forbidden" => StatusCode::FORBIDDEN,
            "invalid-token-request" => StatusCode::BAD_REQUEST,
            "invalid-request" | "invalid-model" | "invalid-json" | "invalid-account" => {
                StatusCode::BAD_REQUEST
            }
            "workflow-unavailable" | "storage-error" | "artifact-io-error" => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            "artifact-integrity" | "invalid-data" => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::CONFLICT,
        };
        (status, Json(self.0.response())).into_response()
    }
}

pub(crate) fn store(state: &AppState) -> Result<&Store, ApiError> {
    state.workflow.as_ref().ok_or_else(|| {
        ApiError(Error::action(
            "workflow-unavailable",
            "The concept workflow service is not active.",
            "Start a daemon with the durable concept runtime configured.",
        ))
    })
}
pub(crate) fn now() -> i64 {
    (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64
}
pub(crate) fn body<T>(body: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    body.map(|Json(value)| value).map_err(|_| {
        ApiError(Error::action(
            "invalid-json",
            "The JSON request does not match this operation.",
            "Use the generated concept request schema and application/json content type.",
        ))
    })
}

pub(crate) fn query<T>(query: Result<Query<T>, QueryRejection>) -> Result<T, ApiError> {
    query.map(|Query(value)| value).map_err(|_| {
        ApiError(Error::action(
            "invalid-request",
            "The query parameters do not match this operation.",
            "Use the documented query parameters and types.",
        ))
    })
}

pub(crate) async fn submit(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<crate::auth::Principal>,
    request: Result<Json<ConceptRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let request = body(request)?;
    let accepted = submit_request(&state, &principal, request).await?;
    Ok((StatusCode::ACCEPTED, Json(accepted)).into_response())
}

pub(crate) async fn submit_request(
    state: &AppState,
    principal: &crate::auth::Principal,
    request: ConceptRequest,
) -> Result<RunAccepted, Error> {
    principal.require_account(&request.account_id)?;
    let store = store(state).map_err(|error| error.0)?;
    let run = if state.account_supervisor.is_some() {
        store.submit_for_account(request, now()).await?
    } else {
        store
            .submit(request, ModelSelection::Current, now())
            .await?
    };
    Ok(RunAccepted {
        run_id: run.id.clone(),
        status: run.status,
        run_url: format!("/api/runs/{}", run.id),
        result_url: format!("/api/runs/{}/result", run.id),
    })
}

pub(crate) async fn result(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<jailgun_workflow::model::FinalResult>, ApiError> {
    Ok(Json(store(&state)?.result(id).await?))
}
pub(crate) async fn pause(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<jailgun_workflow::model::Run>, ApiError> {
    Ok(Json(store(&state)?.pause(id, now()).await?))
}
pub(crate) async fn resume(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    request: Result<Json<ResumeRequest>, JsonRejection>,
) -> Result<Json<jailgun_workflow::model::Run>, ApiError> {
    Ok(Json(
        store(&state)?
            .resume(id, body(request)?.allow_incomplete, now())
            .await?,
    ))
}
pub(crate) async fn cancel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<jailgun_workflow::model::Run>, ApiError> {
    Ok(Json(store(&state)?.cancel(id, now()).await?))
}
pub(crate) async fn attempts(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<jailgun_workflow::model::Attempt>>, ApiError> {
    Ok(Json(store(&state)?.attempts(id).await?))
}
pub(crate) async fn accounts(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<crate::auth::Principal>,
) -> Result<Json<Vec<jailgun_workflow::model::AccountReadiness>>, ApiError> {
    Ok(Json(
        store(&state)?
            .accounts()
            .await?
            .into_iter()
            .filter(|account| principal.permits_account(&account.id))
            .collect(),
    ))
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct EventCursor {
    #[serde(default)]
    after: i64,
}
pub(crate) async fn events(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    cursor: Result<Query<EventCursor>, QueryRejection>,
) -> Result<Json<Vec<jailgun_workflow::model::Event>>, ApiError> {
    let cursor = query(cursor)?;
    if cursor.after < 0 {
        return Err(ApiError(Error::action(
            "invalid-request",
            "The event cursor cannot be negative.",
            "Use zero or the last received sequence number.",
        )));
    }
    Ok(Json(store(&state)?.events(id, cursor.after).await?))
}

pub(crate) async fn artifact(
    State(state): State<Arc<AppState>>,
    Path((id, artifact_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let (artifact, bytes) = store(&state)?.artifact(id, artifact_id).await?;
    let mut response = bytes.into_response();
    let content_type = if artifact.media_type == "application/json" {
        "application/json"
    } else {
        "text/plain; charset=utf-8"
    };
    response
        .headers_mut()
        .insert("content-type", HeaderValue::from_static(content_type));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response.headers_mut().insert(
        "content-security-policy",
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    let name = artifact
        .name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "-_.".contains(*c))
        .collect::<String>();
    response.headers_mut().insert(
        "content-disposition",
        HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")).map_err(|_| {
            ApiError(Error::action(
                "artifact-integrity",
                "The registered artifact name is invalid.",
                "Inspect the registered artifact metadata.",
            ))
        })?,
    );
    Ok(response)
}

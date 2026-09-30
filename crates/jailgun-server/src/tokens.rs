use crate::{
    concepts::{body, now, store, ApiError},
    AppState,
};
use axum::{
    extract::{rejection::JsonRejection, Path, State},
    Json,
};
use jailgun_workflow::model::{CreatedToken, IssueToken, TokenMetadata};
use std::sync::Arc;

pub(crate) async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<TokenMetadata>>, ApiError> {
    Ok(Json(store(&state)?.tokens().await?))
}

pub(crate) async fn issue(
    State(state): State<Arc<AppState>>,
    request: Result<Json<IssueToken>, JsonRejection>,
) -> Result<Json<CreatedToken>, ApiError> {
    Ok(Json(
        store(&state)?.issue_token(body(request)?, now()).await?,
    ))
}

pub(crate) async fn revoke(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<TokenMetadata>, ApiError> {
    Ok(Json(store(&state)?.revoke_token(id, now()).await?))
}

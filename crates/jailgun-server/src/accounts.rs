use crate::{
    concepts::{body, ApiError},
    AppState,
};
use axum::{
    extract::{rejection::JsonRejection, Path, State},
    Json,
};
use jailgun_orchestrator::concept::{service_error, AccountSupervisor};
use jailgun_workflow::{
    model::{AccountSession, ConfirmAccount, ConnectAccount},
    Error,
};
use std::sync::Arc;

fn supervisor(state: &AppState) -> Result<&AccountSupervisor, ApiError> {
    state.account_supervisor.as_ref().ok_or_else(|| {
        Error::action(
            "workflow-unavailable",
            "The account supervisor is not active.",
            "Start the concept daemon.",
        )
        .into()
    })
}

pub(crate) async fn sessions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<AccountSession>>, ApiError> {
    Ok(Json(
        supervisor(&state)?
            .sessions()
            .await
            .map_err(service_error)?,
    ))
}

pub(crate) async fn connect(
    State(state): State<Arc<AppState>>,
    request: Result<Json<ConnectAccount>, JsonRejection>,
) -> Result<Json<Vec<AccountSession>>, ApiError> {
    let service = supervisor(&state)?;
    let account = service.store.connect_account(body(request)?).await?;
    service.start(account).await.map_err(service_error)?;
    Ok(Json(service.sessions().await.map_err(service_error)?))
}

pub(crate) async fn reconnect(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<AccountSession>>, ApiError> {
    let service = supervisor(&state)?;
    service.reconnect(id).await.map_err(service_error)?;
    Ok(Json(service.sessions().await.map_err(service_error)?))
}

pub(crate) async fn confirm(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    request: Result<Json<ConfirmAccount>, JsonRejection>,
) -> Result<Json<Vec<AccountSession>>, ApiError> {
    let service = supervisor(&state)?;
    service
        .confirm(id, body(request)?)
        .await
        .map_err(service_error)?;
    Ok(Json(service.sessions().await.map_err(service_error)?))
}

pub(crate) async fn cancel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<AccountSession>>, ApiError> {
    let service = supervisor(&state)?;
    service.cancel_login(id).await.map_err(service_error)?;
    Ok(Json(service.sessions().await.map_err(service_error)?))
}

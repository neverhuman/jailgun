//! Operator-only shutdown for the exact authenticated daemon instance.
use std::{path::PathBuf, sync::Arc};

use axum::{
    extract::{rejection::JsonRejection, State},
    http::StatusCode,
    Json,
};
use jailgun_core::installation::{ServiceState, ServiceStatus, ServiceStopRequest};
use tokio::sync::watch;

use crate::{
    concepts::{body, ApiError},
    AppState,
};

#[derive(Clone)]
pub struct DaemonControl {
    identity: Arc<ServiceStatus>,
    stopping: watch::Sender<bool>,
}

impl DaemonControl {
    pub fn new(runtime: PathBuf) -> Self {
        Self {
            identity: Arc::new(ServiceStatus {
                state: ServiceState::Running,
                instance_id: Some(uuid::Uuid::new_v4().to_string()),
                runtime,
                pid: Some(std::process::id()),
                version: env!("CARGO_PKG_VERSION").into(),
            }),
            stopping: watch::channel(false).0,
        }
    }

    pub fn is_stopping(&self) -> bool {
        *self.stopping.borrow()
    }

    pub fn status(&self) -> ServiceStatus {
        let mut status = (*self.identity).clone();
        if self.is_stopping() {
            status.state = ServiceState::Stopping;
        }
        status
    }

    pub async fn stopped(&self) {
        let mut receiver = self.stopping.subscribe();
        while !*receiver.borrow_and_update() {
            if receiver.changed().await.is_err() {
                break;
            }
        }
    }

    fn request_stop(&self, instance: &str) -> Result<ServiceStatus, ApiError> {
        if self.identity.instance_id.as_deref() != Some(instance) {
            return Err(ApiError(jailgun_workflow::Error::action(
                "service-instance-changed",
                "This daemon is not the instance observed by the caller.",
                "Inspect service status again before stopping it.",
            )));
        }
        self.stopping.send_replace(true);
        Ok(self.status())
    }
}

fn control(state: &AppState) -> Result<&DaemonControl, ApiError> {
    state.daemon_control.as_ref().ok_or_else(|| {
        ApiError(jailgun_workflow::Error::action(
            "workflow-unavailable",
            "This service does not expose managed daemon shutdown.",
            "Use the process manager that started this service.",
        ))
    })
}

pub(crate) async fn status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ServiceStatus>, ApiError> {
    Ok(Json(control(&state)?.status()))
}

pub(crate) async fn stop(
    State(state): State<Arc<AppState>>,
    request: Result<Json<ServiceStopRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<ServiceStatus>), ApiError> {
    let request = body(request)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(control(&state)?.request_stop(&request.instance_id)?),
    ))
}

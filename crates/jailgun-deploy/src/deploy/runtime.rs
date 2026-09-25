use jailgun_core::{EventKind, JailgunEvent, Severity};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::sync::broadcast;

use crate::{
    ci::{CiState, CiTracker},
    job::{JobHandle, JobStatus, RemoteJobBackend},
};

use super::{
    events::{phase_str, publish, publish_ci_progress, publish_status_progress},
    DeployError, DeployRequest,
};

pub(super) async fn poll_until_terminal<J>(
    job: &mut J,
    handle: &JobHandle,
    req: &DeployRequest,
    events: &broadcast::Sender<JailgunEvent>,
) -> Result<JobStatus, DeployError>
where
    J: RemoteJobBackend + Send,
{
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs(req.status_max_minutes as u64 * 60);
    let poll_interval = std::time::Duration::from_secs(req.status_poll_seconds as u64);
    let mut consecutive_errors: u8 = 0;
    let mut last_seen: JobStatus = JobStatus::default();
    loop {
        if std::time::Instant::now() >= deadline {
            publish(
                events,
                JailgunEvent::new(
                    req.run_id.clone(),
                    EventKind::DeployFinished,
                    "deploy timeout",
                )
                .with_tab(req.tab_id)
                .with_severity(Severity::Error)
                .with_field("outcome", "timed-out")
                .with_field("reason", "status_max_minutes exceeded"),
            );
            return Err(DeployError::Timeout(req.status_max_minutes));
        }
        tokio::time::sleep(poll_interval).await;
        match job.fetch_status(handle).await {
            Ok(status) => {
                consecutive_errors = 0;
                publish_status_progress(events, req, &status);
                if status.phase.is_terminal() {
                    return Ok(status);
                }
                last_seen = status;
            }
            Err(error) => {
                consecutive_errors = consecutive_errors.saturating_add(1);
                publish_status_fetch_error(events, req, consecutive_errors, &last_seen, &error);
                if consecutive_errors >= 5 {
                    return Err(error);
                }
            }
        }
    }
}

pub(super) async fn track_ci<C: CiTracker + Send>(
    ci: &mut C,
    status: &JobStatus,
    req: &DeployRequest,
    events: &broadcast::Sender<JailgunEvent>,
) -> CiState {
    let Some(commit_sha) = status.post_head.as_ref() else {
        return CiState::Unknown;
    };
    let interval = std::time::Duration::from_secs(req.ci_poll_seconds as u64);
    let mut last_observed = CiState::Unknown;
    for attempt in 1..=req.ci_max_attempts {
        match ci.check(commit_sha, &req.ci_branch).await {
            Ok(state) => {
                publish_ci_progress(events, req, &state, attempt);
                if state.is_terminal() {
                    return with_ci_failure_log(ci, state).await;
                }
                last_observed = state;
            }
            Err(error) => publish_ci_error(events, req, attempt, &error),
        }
        tokio::time::sleep(interval).await;
    }
    last_observed
}

pub(super) fn timestamp_now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

async fn with_ci_failure_log<C: CiTracker + Send>(ci: &mut C, state: CiState) -> CiState {
    if let CiState::Failed { run_id, .. } = &state {
        let excerpt = ci.capture_failure_log(run_id, 16 * 1024).await.ok();
        let mut final_state = state.clone();
        if let CiState::Failed { log_excerpt, .. } = &mut final_state {
            *log_excerpt = excerpt;
        }
        return final_state;
    }
    state
}

fn publish_status_fetch_error<E: std::fmt::Debug>(
    events: &broadcast::Sender<JailgunEvent>,
    req: &DeployRequest,
    attempts: u8,
    last_seen: &JobStatus,
    error: &E,
) {
    tracing::warn!(?error, attempts, "status fetch failed");
    publish(
        events,
        JailgunEvent::new(
            req.run_id.clone(),
            EventKind::RemoteSafety,
            "status fetch error",
        )
        .with_tab(req.tab_id)
        .with_severity(Severity::Warn)
        .with_field("phase", "status-fetch-error")
        .with_field("attempts", attempts.to_string())
        .with_field("last_phase", phase_str(&last_seen.phase).to_string()),
    );
}

fn publish_ci_error<E: std::fmt::Debug>(
    events: &broadcast::Sender<JailgunEvent>,
    req: &DeployRequest,
    attempt: u32,
    error: &E,
) {
    tracing::warn!(?error, attempt, "CI check failed");
    publish(
        events,
        JailgunEvent::new(
            req.run_id.clone(),
            EventKind::RemoteSafety,
            "ci tracker transient error",
        )
        .with_tab(req.tab_id)
        .with_severity(Severity::Warn)
        .with_field("phase", "ci-error")
        .with_field("attempt", attempt.to_string()),
    );
}

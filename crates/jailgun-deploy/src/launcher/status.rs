use crate::job::JobStatus;

/// Parse the `status.json` blob emitted by the launcher into a typed
/// [`JobStatus`]. Unknown / future fields are preserved under `raw` for
/// forensic visibility.
pub fn parse_status_json(text: &str) -> Result<JobStatus, ParseError> {
    let raw: serde_json::Value = serde_json::from_str(text)?;
    let mut status: JobStatus = match serde_json::from_value(raw.clone()) {
        Ok(typed) => typed,
        Err(error) => {
            tracing::warn!(
                ?error,
                "launcher status JSON did not match known schema; using empty typed status with raw payload preserved"
            );
            JobStatus::default()
        }
    };
    status.raw = raw;
    Ok(status)
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("status JSON parse failed: {0}")]
    Json(#[from] serde_json::Error),
}

use jailgun_workflow::ErrorResponse;

#[derive(Debug)]
pub struct Failure(pub ErrorResponse);
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {}\n{}",
            self.0.code, self.0.message, self.0.next_action
        )
    }
}
impl std::error::Error for Failure {}
pub fn failure(code: &str, message: impl Into<String>, action: impl Into<String>) -> anyhow::Error {
    Failure(ErrorResponse {
        code: code.into(),
        message: message.into(),
        next_action: action.into(),
    })
    .into()
}
pub fn installation(error: jailgun_core::managed_installation::InstallationError) -> anyhow::Error {
    failure(error.code(), error.to_string(), error.next_action())
}
pub fn response(error: &anyhow::Error) -> ErrorResponse {
    if let Some(failure) = error.downcast_ref::<Failure>() {
        return failure.0.clone();
    }
    if let Some(error) = error.downcast_ref::<jailgun_workflow::Error>() {
        return error.response();
    }
    ErrorResponse {
        code: "operation-failed".into(),
        message: error.to_string(),
        next_action: "Inspect the command options and local diagnostics, then retry.".into(),
    }
}

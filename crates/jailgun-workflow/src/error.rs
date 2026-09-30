use serde::{Deserialize, Serialize};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{code}: {message}; {next_action}")]
    Action {
        code: &'static str,
        message: String,
        next_action: &'static str,
    },
    #[error("database operation failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("private storage operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid stored data: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ErrorResponse {
    pub code: String,
    pub message: String,
    pub next_action: String,
}

impl Error {
    pub fn action(
        code: &'static str,
        message: impl Into<String>,
        next_action: &'static str,
    ) -> Self {
        Self::Action {
            code,
            message: message.into(),
            next_action,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Action { code, .. } => code,
            Self::Database(_) => "storage-error",
            Self::Io(_) => "artifact-io-error",
            Self::Json(_) => "invalid-data",
        }
    }

    /// Do not leak filesystem paths or SQL diagnostics through public interfaces.
    pub fn response(&self) -> ErrorResponse {
        match self {
            Self::Action {
                code,
                message,
                next_action,
            } => ErrorResponse {
                code: (*code).into(),
                message: message.clone(),
                next_action: (*next_action).into(),
            },
            _ => ErrorResponse {
                code: self.code().into(),
                message: "The private storage operation failed.".into(),
                next_action:
                    "Check local storage permissions, available space, and daemon diagnostics."
                        .into(),
            },
        }
    }
}

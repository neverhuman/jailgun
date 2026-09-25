use thiserror::Error;

use crate::agent_error::{AgentError, AgentErrorExt};

#[derive(Debug, Error)]
pub enum TarError {
    #[error("could not open archive {path}: {source}")]
    Open {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("could not read archive {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("archive contains unsafe entry: {0}")]
    UnsafeEntry(String),
    #[error("archive has no entries")]
    Empty,
    #[error("archive must contain exactly one top-level directory; found: {0}")]
    MultipleTopLevels(String),
    #[error("archive must contain files under its top-level directory")]
    MissingChildEntry,
}

impl AgentErrorExt for TarError {
    fn agent_error(&self) -> AgentError {
        AgentError::new(
            "tar-validation",
            "validate downloaded source archive",
            self.to_string(),
            vec![
                "confirm the downloaded file is a .tar.gz archive",
                "ensure the archive has one top-level directory",
                "remove absolute paths, parent traversal, and .git entries",
            ],
            "docs/testing.md",
            "rerun `cargo test -p jailgun-core tarball`",
        )
    }
}

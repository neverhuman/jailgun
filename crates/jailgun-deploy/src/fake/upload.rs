use std::path::Path;

use async_trait::async_trait;

use crate::{deploy::DeployError, upload::RemoteUploadBackend};

use super::FakeOutcome;

pub struct FakeRemoteUpload {
    outcome: FakeOutcome,
}

impl FakeRemoteUpload {
    pub fn new(outcome: FakeOutcome) -> Self {
        Self { outcome }
    }
}

#[async_trait]
impl RemoteUploadBackend for FakeRemoteUpload {
    async fn ensure_remote_dir(&mut self, _remote_dir: &str) -> Result<(), DeployError> {
        Ok(())
    }

    async fn upload_archive(&mut self, _local: &Path, _remote: &str) -> Result<(), DeployError> {
        Ok(())
    }

    async fn remote_sha256(&mut self, _remote: &str) -> Result<String, DeployError> {
        match self.outcome {
            FakeOutcome::ShaMismatch => Ok("0".repeat(64)),
            _ => match std::env::var("JAILGUN_FAKE_LOCAL_SHA") {
                Ok(value) => Ok(value),
                Err(std::env::VarError::NotPresent) => Ok("a".repeat(64)),
                Err(std::env::VarError::NotUnicode(_)) => Err(DeployError::Sha256(
                    "JAILGUN_FAKE_LOCAL_SHA is not UTF-8".into(),
                )),
            },
        }
    }

    async fn remove_remote_file(&mut self, _remote: &str) -> Result<(), DeployError> {
        Ok(())
    }
}

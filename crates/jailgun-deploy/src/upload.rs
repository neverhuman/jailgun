//! Remote upload backend trait + payload types.

use std::path::Path;

use async_trait::async_trait;

use crate::deploy::DeployError;

/// Side-effects required to put a `.tar.gz` archive on the remote host and
/// verify it landed intact. All implementations must be safe to call multiple
/// times for the same `remote_path` (idempotent on the directory-prep call,
/// overwrite-on-collision for the archive upload).
#[async_trait]
pub trait RemoteUploadBackend {
    async fn ensure_remote_dir(&mut self, remote_dir: &str) -> Result<(), DeployError>;
    async fn upload_archive(
        &mut self,
        local_path: &Path,
        remote_path: &str,
    ) -> Result<(), DeployError>;
    async fn remote_sha256(&mut self, remote_path: &str) -> Result<String, DeployError>;
    async fn remove_remote_file(&mut self, remote_path: &str) -> Result<(), DeployError>;
}

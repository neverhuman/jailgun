//! Atomic envelope writer for the JMCP outbox.
//!
//! The contract:
//! - One envelope per JSON file.
//! - Filename = `<envelope_id>.json` (matches the JPCM ID pattern).
//! - Write to `<inbox>/tmp/<envelope_id>.json.partial`, fsync, then rename
//!   into `<inbox>/<envelope_id>.json`. Renames are atomic on POSIX so the
//!   bridge never sees a half-written file.
//! - File mode `0600` on the final file; the `tmp/` directory itself is
//!   mode `0700`.
//! - Inbox dir is created on demand. Parents must already exist.

use std::{
    io,
    path::{Path, PathBuf},
};

use thiserror::Error;
use tokio::{fs, io::AsyncWriteExt};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::envelope::JmcpEnvelope;

#[derive(Debug, Error)]
pub enum JmcpInboxError {
    #[error("could not create JMCP inbox directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not serialize JMCP envelope: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("could not write JMCP envelope to {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not finalize JMCP envelope at {path}: {source}")]
    Finalize {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Handle to a JMCP outbox directory. Construction is cheap and does no IO.
#[derive(Debug, Clone)]
pub struct JmcpInbox {
    root: PathBuf,
}

impl JmcpInbox {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn tmp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }

    /// Ensure `root/` and `root/tmp/` exist. Idempotent; safe to call from
    /// every write.
    pub async fn ensure_layout(&self) -> Result<(), JmcpInboxError> {
        ensure_dir(&self.root, 0o700).await?;
        ensure_dir(&self.tmp_dir(), 0o700).await?;
        Ok(())
    }

    /// Serialize the envelope, write to tmp, fsync, rename into the inbox.
    /// Returns the final destination path on success.
    pub async fn write_envelope(&self, envelope: &JmcpEnvelope) -> Result<PathBuf, JmcpInboxError> {
        self.ensure_layout().await?;

        let filename = format!("{}.json", envelope.envelope_id);
        let final_path = self.root.join(&filename);
        let tmp_path = self.tmp_dir().join(format!("{filename}.partial"));

        let body = serde_json::to_vec_pretty(envelope)?;

        let mut tmp_file =
            fs::File::create(&tmp_path)
                .await
                .map_err(|source| JmcpInboxError::Write {
                    path: tmp_path.clone(),
                    source,
                })?;
        tmp_file
            .write_all(&body)
            .await
            .map_err(|source| JmcpInboxError::Write {
                path: tmp_path.clone(),
                source,
            })?;
        tmp_file
            .write_all(b"\n")
            .await
            .map_err(|source| JmcpInboxError::Write {
                path: tmp_path.clone(),
                source,
            })?;
        tmp_file
            .flush()
            .await
            .map_err(|source| JmcpInboxError::Write {
                path: tmp_path.clone(),
                source,
            })?;
        tmp_file
            .sync_all()
            .await
            .map_err(|source| JmcpInboxError::Write {
                path: tmp_path.clone(),
                source,
            })?;
        drop(tmp_file);

        set_mode(&tmp_path, 0o600)
            .await
            .map_err(|source| JmcpInboxError::Finalize {
                path: tmp_path.clone(),
                source,
            })?;

        fs::rename(&tmp_path, &final_path)
            .await
            .map_err(|source| JmcpInboxError::Finalize {
                path: final_path.clone(),
                source,
            })?;

        Ok(final_path)
    }
}

async fn ensure_dir(path: &Path, mode: u32) -> Result<(), JmcpInboxError> {
    fs::create_dir_all(path)
        .await
        .map_err(|source| JmcpInboxError::CreateDir {
            path: path.to_path_buf(),
            source,
        })?;
    set_mode(path, mode)
        .await
        .map_err(|source| JmcpInboxError::CreateDir {
            path: path.to_path_buf(),
            source,
        })
}

#[cfg(unix)]
async fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    let permissions = std::fs::Permissions::from_mode(mode);
    fs::set_permissions(path, permissions).await
}

#[cfg(not(unix))]
async fn set_mode(_path: &Path, _mode: u32) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::{NotifyTextPayload, Payload, Routing, TaskRef};

    fn sample_envelope() -> JmcpEnvelope {
        JmcpEnvelope::new(
            Payload::NotifyText(NotifyTextPayload {
                title: "hi".into(),
                summary_emoji: "✨".into(),
                body_markdown: "hello".into(),
            }),
            TaskRef::for_run("run-A", Some(1)),
            Routing::notify_user(),
        )
    }

    #[tokio::test]
    async fn writes_envelope_into_inbox_with_mode_0600() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let inbox = JmcpInbox::new(tmp.path().join("inbox"));
        let envelope = sample_envelope();
        let path = inbox.write_envelope(&envelope).await.expect("write");

        assert!(path.exists());
        assert!(path.starts_with(inbox.root()));

        #[cfg(unix)]
        {
            let metadata = std::fs::metadata(&path).expect("metadata");
            assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        }

        let body = tokio::fs::read_to_string(&path).await.expect("read back");
        let back: JmcpEnvelope = serde_json::from_str(&body).expect("decode");
        assert_eq!(back, envelope);
    }

    #[tokio::test]
    async fn write_is_atomic_no_partial_file_left_behind() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let inbox = JmcpInbox::new(tmp.path().join("inbox"));
        let envelope = sample_envelope();
        inbox.write_envelope(&envelope).await.expect("write");

        let mut entries = tokio::fs::read_dir(inbox.root().join("tmp"))
            .await
            .expect("read tmp dir");
        if let Ok(Some(entry)) = entries.next_entry().await {
            panic!(
                "unexpected partial left in tmp/: {}",
                entry.path().display()
            );
        }
    }
}

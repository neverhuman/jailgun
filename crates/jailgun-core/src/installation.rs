//! Private installation credentials and one daemon owner per runtime root.
use std::{
    fs::File,
    path::{Path, PathBuf},
};

use crate::browser_registry::{
    ensure_private_dir,
    storage::{atomic_write, private_file},
    BrowserRegistryError,
};

pub struct Installation {
    root: PathBuf,
    _owner: File,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DaemonAddress {
    pub url: String,
    pub pid: u32,
    pub version: String,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ServiceState {
    Running,
    Stopping,
    Stopped,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ServiceStatus {
    pub state: ServiceState,
    pub instance_id: Option<String>,
    pub runtime: PathBuf,
    pub pid: Option<u32>,
    pub version: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceStopRequest {
    pub instance_id: String,
}

impl Installation {
    /// Inspect an existing lock without creating credentials, files or directories.
    /// A stale descriptor or PID does not establish ownership.
    pub fn is_owned(root: &Path) -> std::io::Result<bool> {
        let lock = root.join("daemon.lock");
        let metadata = match std::fs::symlink_metadata(&lock) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        if !metadata.is_file() {
            return Err(std::io::Error::other("daemon lock must be a regular file"));
        }
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let opened = file.metadata()?;
            if metadata.dev() != opened.dev() || metadata.ino() != opened.ino() {
                return Err(std::io::Error::other(
                    "daemon lock changed during inspection",
                ));
            }
        }
        match file.try_lock() {
            Ok(()) => Ok(false),
            Err(std::fs::TryLockError::WouldBlock) => Ok(true),
            Err(std::fs::TryLockError::Error(error)) => Err(error),
        }
    }

    pub fn record_address(
        &self,
        address: std::net::SocketAddr,
    ) -> Result<(), BrowserRegistryError> {
        let descriptor = DaemonAddress {
            url: format!("http://{address}"),
            pid: std::process::id(),
            version: env!("CARGO_PKG_VERSION").into(),
        };
        atomic_write(
            &self.root.join("daemon.json"),
            &serde_json::to_vec(&descriptor).expect("daemon descriptor"),
        )
    }

    pub fn acquire(root: &Path) -> Result<Self, BrowserRegistryError> {
        ensure_private_dir(root)?;
        let lock_path = root.join("daemon.lock");
        let owner = private_file(&lock_path, false)?;
        owner
            .try_lock()
            .map_err(|source| BrowserRegistryError::Lock {
                path: lock_path.display().to_string(),
                source: std::io::Error::other(format!(
                    "daemon-already-owned: another service may own this runtime: {source}"
                )),
            })?;
        Ok(Self {
            root: root.to_path_buf(),
            _owner: owner,
        })
    }

    pub fn operator_token(&self) -> Result<String, BrowserRegistryError> {
        let path = self.root.join("operator-token");
        match std::fs::read_to_string(&path) {
            Ok(value) => {
                validate_operator_token(value.trim()).map_err(|reason| {
                    BrowserRegistryError::Write {
                        path: path.display().to_string(),
                        source: std::io::Error::new(std::io::ErrorKind::InvalidData, reason),
                    }
                })?;
                // Also correct permissions on imported pre-existing credentials.
                let _file = private_file(&path, false)?;
                Ok(value.trim().to_string())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let token = format!(
                    "{}{}",
                    uuid::Uuid::new_v4().simple(),
                    uuid::Uuid::new_v4().simple()
                );
                atomic_write(&path, format!("{token}\n").as_bytes())?;
                Ok(token)
            }
            Err(source) => Err(BrowserRegistryError::Read {
                path: path.display().to_string(),
                source,
            }),
        }
    }
}

pub fn validate_operator_token(value: &str) -> Result<(), &'static str> {
    if value.len() < 32
        || value.chars().any(char::is_whitespace)
        || value.to_ascii_lowercase().contains("example")
        || value.to_ascii_lowercase().contains("replace")
    {
        Err("operator-token-invalid: use a random credential of at least 32 characters, or omit the token option to generate one")
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ownership_and_credentials_survive_restart() {
        let root = tempfile::tempdir().unwrap();
        let install = Installation::acquire(root.path()).unwrap();
        assert!(Installation::acquire(root.path()).is_err());
        let token = install.operator_token().unwrap();
        assert_eq!(token.len(), 64);
        drop(install);
        let reopened = Installation::acquire(root.path()).unwrap();
        assert_eq!(token, reopened.operator_token().unwrap());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for name in ["operator-token", "daemon.lock"] {
                assert_eq!(
                    std::fs::metadata(root.path().join(name))
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    0o600
                );
            }
        }
    }

    #[test]
    fn example_credentials_are_rejected() {
        for value in [
            "",
            "secret",
            "replace-this-with-a-real-random-token",
            "example-token-that-is-over-32-characters",
            "this token contains whitespace and is long",
        ] {
            assert!(validate_operator_token(value).is_err());
        }
    }

    #[test]
    fn ownership_probe_is_read_only_and_rejects_links() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("runtime");
        assert!(!Installation::is_owned(&root).unwrap());
        assert!(!root.exists());
        let owner = Installation::acquire(&root).unwrap();
        assert!(Installation::is_owned(&root).unwrap());
        drop(owner);
        assert!(!Installation::is_owned(&root).unwrap());
        assert!(!root.join("operator-token").exists());
        #[cfg(unix)]
        {
            let other = parent.path().join("linked");
            std::fs::create_dir(&other).unwrap();
            std::os::unix::fs::symlink(root.join("daemon.lock"), other.join("daemon.lock"))
                .unwrap();
            assert!(Installation::is_owned(&other).is_err());
        }
    }
}

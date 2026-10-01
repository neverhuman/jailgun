//! Managed application inventory and process-use locks; runtime data is separate.
mod inventory;
mod removal;
pub use removal::{uninstall, Removal};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
};

#[derive(Debug, thiserror::Error)]
pub enum InstallationError {
    #[error("Managed application inventory or paths do not match.")]
    Invalid,
    #[error("Another process is using or removing this application.")]
    Busy,
    #[error("An installed release does not support application-use locking.")]
    LockUnsupported,
    #[error(transparent)]
    Io(#[from] io::Error),
}
pub type Result<T> = std::result::Result<T, InstallationError>;
impl InstallationError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Invalid => "installation-invalid",
            Self::Busy => "installation-busy",
            Self::LockUnsupported => "installation-lock-unsupported",
            Self::Io(_) => "installation-unavailable",
        }
    }
    pub fn next_action(&self) -> &'static str {
        match self {
            Self::Busy => "Stop services using this installation before removing application files.",
            Self::LockUnsupported => "Stop and inspect older installations before removal; their processes cannot be assumed to hold a use lock.",
            _ => "Inspect installation paths and metadata, or reinstall before changing application files.",
        }
    }
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReleaseRecord {
    pub archive_sha256: String,
    pub manifest_sha256: String,
    #[serde(default)]
    pub usage_lock_version: u32,
}
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
struct Record {
    schema_version: u32,
    application: String,
    prefix: PathBuf,
    releases: BTreeMap<String, ReleaseRecord>,
}
pub struct ManagedInstallation {
    pub prefix: PathBuf,
    pub base: PathBuf,
    pub releases: BTreeMap<String, ReleaseRecord>,
}

impl ManagedInstallation {
    /// Manual archives and development binaries have no managed installation.
    /// A recognized managed layout must have valid custody metadata.
    pub fn for_executable(executable: &Path) -> Result<Option<Self>> {
        let executable = executable.canonicalize()?;
        let Some(bin) = executable
            .parent()
            .filter(|path| path.file_name().is_some_and(|name| name == "bin"))
        else {
            return Ok(None);
        };
        let Some(release) = bin.parent() else {
            return Ok(None);
        };
        let Some(releases) = release
            .parent()
            .filter(|path| path.file_name().is_some_and(|name| name == "releases"))
        else {
            return Ok(None);
        };
        let Some(base) = releases
            .parent()
            .filter(|path| path.file_name().is_some_and(|name| name == "jailgun"))
        else {
            return Ok(None);
        };
        let Some(lib) = base
            .parent()
            .filter(|path| path.file_name().is_some_and(|name| name == "lib"))
        else {
            return Ok(None);
        };
        let prefix = lib.parent().ok_or_else(invalid)?;
        let installation = Self::load(prefix)?;
        let name = release
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(invalid)?;
        if !installation.releases.contains_key(name) {
            return Err(invalid());
        }
        directory(release)?;
        directory(bin)?;
        Ok(Some(installation))
    }

    pub fn load(prefix: &Path) -> Result<Self> {
        let prefix = prefix.canonicalize()?;
        let base = prefix.join("lib/jailgun");
        for path in [
            prefix.join("bin"),
            prefix.join("lib"),
            base.clone(),
            base.join("releases"),
        ] {
            directory(&path)?;
        }
        let record: Record = serde_json::from_slice(&regular_bytes(
            &base.join("installation.json"),
            8 * 1024 * 1024,
        )?)
        .map_err(|_| invalid())?;
        Self::from_record(prefix, record)
    }

    fn from_record(prefix: PathBuf, record: Record) -> Result<Self> {
        if record.schema_version != 1
            || record.application != "jailgun"
            || record.prefix != prefix
            || record.releases.is_empty()
        {
            return Err(invalid());
        }
        for (name, release) in &record.releases {
            if !release_name(name)
                || !digest(&release.archive_sha256)
                || !digest(&release.manifest_sha256)
                || !name.ends_with(&release.archive_sha256)
            {
                return Err(invalid());
            }
        }
        Ok(Self {
            base: prefix.join("lib/jailgun"),
            prefix,
            releases: record.releases,
        })
    }

    /// Keep the guard alive until every operation using this installation ends.
    pub fn shared_use(&self) -> Result<File> {
        let file = regular_file(&self.base.join(".usage.lock"))?;
        file.try_lock_shared().map_err(lock_error)?;
        if self.base.join(".uninstall.json").symlink_metadata().is_ok() {
            return Err(InstallationError::Busy);
        }
        Ok(file)
    }

    /// The caller must also hold the installer directory lock before changing inventory.
    pub fn exclusive_use(&self) -> Result<File> {
        self.require_guarded_releases()?;
        let file = regular_file(&self.base.join(".usage.lock"))?;
        file.try_lock().map_err(lock_error)?;
        Ok(file)
    }

    fn require_guarded_releases(&self) -> Result<()> {
        if self
            .releases
            .values()
            .any(|release| release.usage_lock_version != 1)
        {
            return Err(InstallationError::LockUnsupported);
        }
        Ok(())
    }

    pub fn active_binary(&self, name: &str) -> Result<PathBuf> {
        if !["jailgun", "jailhard"].contains(&name) {
            return Err(invalid());
        }
        let public = self.prefix.join("bin").join(name);
        if fs::read_link(&public)? != Path::new(&format!("../lib/jailgun/current/bin/{name}")) {
            return Err(invalid());
        }
        let current = fs::read_link(self.base.join("current"))?;
        let relative = current
            .to_str()
            .and_then(|path| path.strip_prefix("releases/"))
            .ok_or_else(invalid)?;
        if !release_name(relative) || !self.releases.contains_key(relative) {
            return Err(invalid());
        }
        let release = self.base.join(&current);
        directory(&release)?;
        directory(&release.join("bin"))?;
        let actual = release.join("bin").join(name);
        regular_file(&actual)?;
        if public.canonicalize()? != actual {
            return Err(invalid());
        }
        Ok(public)
    }
}

pub fn current_use() -> Result<Option<File>> {
    ManagedInstallation::for_executable(&std::env::current_exe()?)?
        .map(|installation| installation.shared_use())
        .transpose()
}

pub(crate) fn regular_file(path: &Path) -> Result<File> {
    let before = fs::symlink_metadata(path)?;
    if !before.is_file() {
        return Err(invalid());
    }
    let file = OpenOptions::new().read(true).open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let after = file.metadata()?;
        if before.dev() != after.dev() || before.ino() != after.ino() {
            return Err(invalid());
        }
    }
    Ok(file)
}
pub(crate) fn regular_bytes(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = regular_file(path)?;
    if file.metadata()?.len() > limit {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid());
    }
    Ok(bytes)
}
fn directory(path: &Path) -> Result<()> {
    if !fs::symlink_metadata(path)?.is_dir() {
        return Err(invalid());
    }
    Ok(())
}
fn release_name(name: &str) -> bool {
    name.len() <= 240
        && name.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-".contains(&byte))
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn invalid() -> InstallationError {
    InstallationError::Invalid
}
fn lock_error(error: fs::TryLockError) -> InstallationError {
    match error {
        fs::TryLockError::WouldBlock => InstallationError::Busy,
        fs::TryLockError::Error(error) => InstallationError::Io(error),
    }
}

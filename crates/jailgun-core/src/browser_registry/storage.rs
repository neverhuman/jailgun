use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use super::{BrowserRegistryError, DEFAULT_BROWSER_REGISTRY_RELATIVE_PATH};

pub fn default_registry_path() -> PathBuf {
    home_dir_or_current().join(DEFAULT_BROWSER_REGISTRY_RELATIVE_PATH)
}

pub fn ensure_private_dir(path: &Path) -> Result<(), BrowserRegistryError> {
    fs::create_dir_all(path).map_err(|source| BrowserRegistryError::CreatePrivateDir {
        path: path.display().to_string(),
        source,
    })?;
    set_private_dir_permissions(path)?;
    Ok(())
}

pub(super) fn home_dir_or_current() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(super) fn registry_tmp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("browser-profiles.json");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    path.with_file_name(format!(".{file_name}.tmp.{}.{}", std::process::id(), nanos))
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &Path) -> Result<(), BrowserRegistryError> {
    use std::os::unix::fs::PermissionsExt;

    let permissions = fs::Permissions::from_mode(0o700);
    fs::set_permissions(path, permissions).map_err(|source| {
        BrowserRegistryError::CreatePrivateDir {
            path: path.display().to_string(),
            source,
        }
    })
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_path: &Path) -> Result<(), BrowserRegistryError> {
    Ok(())
}

#[cfg(unix)]
pub(super) fn set_private_file_permissions(path: &Path) -> Result<(), BrowserRegistryError> {
    use std::os::unix::fs::PermissionsExt;

    let permissions = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, permissions).map_err(|source| BrowserRegistryError::Write {
        path: path.display().to_string(),
        source,
    })
}

#[cfg(not(unix))]
pub(super) fn set_private_file_permissions(_path: &Path) -> Result<(), BrowserRegistryError> {
    Ok(())
}

use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use super::{lock::InterprocessFileLock, BrowserLeaseState, BrowserRegistryError};
use crate::browser_registry::storage::{
    ensure_private_dir, registry_tmp_path, set_private_file_permissions,
};

pub(super) fn release_lease_ids(
    lease_path: &Path,
    lease_ids: &[String],
) -> Result<(), BrowserRegistryError> {
    let wanted = lease_ids.iter().cloned().collect::<BTreeSet<_>>();
    with_locked_leases(lease_path, |state| {
        let now = unix_seconds();
        state.purge_stale(now);
        state
            .leases
            .retain(|lease| !wanted.contains(&lease.lease_id));
        Ok(())
    })
}

pub(super) fn with_locked_leases<T>(
    lease_path: &Path,
    update: impl FnOnce(&mut BrowserLeaseState) -> Result<T, BrowserRegistryError>,
) -> Result<T, BrowserRegistryError> {
    if let Some(parent) = lease_path.parent() {
        ensure_private_dir(parent)?;
    }
    let lock_path = lock_path_for_lease_path(lease_path);
    let lock_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|source| BrowserRegistryError::Lock {
            path: lock_path.display().to_string(),
            source,
        })?;
    set_private_file_permissions(&lock_path)?;
    let _guard = InterprocessFileLock::lock(lock_file, &lock_path)?;

    let mut state = load_lease_state(lease_path)?;
    let result = update(&mut state)?;
    save_lease_state(lease_path, &state)?;
    Ok(result)
}

pub(super) fn load_lease_state(path: &Path) -> Result<BrowserLeaseState, BrowserRegistryError> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|source| BrowserRegistryError::Parse {
            path: path.display().to_string(),
            source,
        }),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(BrowserLeaseState::default()),
        Err(source) => Err(BrowserRegistryError::Read {
            path: path.display().to_string(),
            source,
        }),
    }
}

pub(super) fn save_lease_state(
    path: &Path,
    state: &BrowserLeaseState,
) -> Result<(), BrowserRegistryError> {
    let bytes = serde_json::to_vec_pretty(state).map_err(|source| BrowserRegistryError::Write {
        path: path.display().to_string(),
        source: io::Error::new(io::ErrorKind::InvalidData, source),
    })?;
    let tmp_path = registry_tmp_path(path);
    fs::write(&tmp_path, bytes).map_err(|source| BrowserRegistryError::Write {
        path: tmp_path.display().to_string(),
        source,
    })?;
    set_private_file_permissions(&tmp_path)?;
    fs::rename(&tmp_path, path).map_err(|source| BrowserRegistryError::Write {
        path: path.display().to_string(),
        source,
    })?;
    set_private_file_permissions(path)?;
    Ok(())
}

pub(super) fn lease_path_for_registry(registry_path: &Path) -> PathBuf {
    let file_name = registry_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("browser-profiles.json");
    registry_path.with_file_name(format!("{file_name}.leases.json"))
}

pub(super) fn lock_path_for_lease_path(lease_path: &Path) -> PathBuf {
    let file_name = lease_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("browser-profiles.json.leases.json");
    lease_path.with_file_name(format!(".{file_name}.lock"))
}

pub(super) fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs()
}

pub(super) fn default_lease_state_version() -> u16 {
    1
}

#[cfg(target_os = "linux")]
pub(super) fn owner_process_is_alive(pid: u32) -> bool {
    pid == std::process::id() || PathBuf::from(format!("/proc/{pid}")).exists()
}

#[cfg(not(target_os = "linux"))]
pub(super) fn owner_process_is_alive(_pid: u32) -> bool {
    true
}

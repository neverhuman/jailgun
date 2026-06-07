use std::{
    fs::File,
    io,
    path::{Path, PathBuf},
};

use super::BrowserRegistryError;

pub(super) struct InterprocessFileLock {
    file: File,
    path: PathBuf,
}

impl InterprocessFileLock {
    pub(super) fn lock(file: File, path: &Path) -> Result<Self, BrowserRegistryError> {
        lock_file_exclusive(&file, path)?;
        Ok(Self {
            file,
            path: path.to_path_buf(),
        })
    }
}

impl Drop for InterprocessFileLock {
    fn drop(&mut self) {
        let _ = unlock_file(&self.file, &self.path);
    }
}

#[cfg(unix)]
fn lock_file_exclusive(file: &File, path: &Path) -> Result<(), BrowserRegistryError> {
    use std::{os::fd::AsRawFd, os::raw::c_int};

    extern "C" {
        fn flock(fd: c_int, operation: c_int) -> c_int;
    }

    const LOCK_EX: c_int = 2;
    // SAFETY: `file.as_raw_fd()` is borrowed from a live `File` for the duration
    // of this call, and `LOCK_EX` is a valid `flock(2)` operation constant.
    let rc = unsafe { flock(file.as_raw_fd(), LOCK_EX) };
    if rc == 0 {
        Ok(())
    } else {
        Err(BrowserRegistryError::Lock {
            path: path.display().to_string(),
            source: io::Error::last_os_error(),
        })
    }
}

#[cfg(unix)]
fn unlock_file(file: &File, path: &Path) -> Result<(), BrowserRegistryError> {
    use std::{os::fd::AsRawFd, os::raw::c_int};

    extern "C" {
        fn flock(fd: c_int, operation: c_int) -> c_int;
    }

    const LOCK_UN: c_int = 8;
    // SAFETY: `file.as_raw_fd()` is borrowed from a live `File` for the duration
    // of this call, and `LOCK_UN` is a valid `flock(2)` operation constant.
    let rc = unsafe { flock(file.as_raw_fd(), LOCK_UN) };
    if rc == 0 {
        Ok(())
    } else {
        Err(BrowserRegistryError::Lock {
            path: path.display().to_string(),
            source: io::Error::last_os_error(),
        })
    }
}

#[cfg(not(unix))]
fn lock_file_exclusive(_file: &File, _path: &Path) -> Result<(), BrowserRegistryError> {
    Ok(())
}

#[cfg(not(unix))]
fn unlock_file(_file: &File, _path: &Path) -> Result<(), BrowserRegistryError> {
    Ok(())
}

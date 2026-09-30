//! One durable installation owner per browser profile, shared by migration and registration.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
};

pub const MARKER: &str = ".jailgun-workflow-owner";

/// Publish a fully written owner without replacing another claim. Repeating the
/// same claim is safe after a transaction failure; links and foreign claims fail.
pub fn claim(profile: &Path, runtime: &Path) -> io::Result<()> {
    let root = runtime
        .to_str()
        .filter(|root| !root.chars().any(char::is_control))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "runtime owner must be a UTF-8 path without control characters",
            )
        })?;
    if !profile.is_absolute() || !runtime.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "profile ownership requires absolute paths",
        ));
    }
    let marker = profile.join(MARKER);
    let expected = format!("{root}\n");
    if matches_owner(&marker, expected.as_bytes())? {
        return File::open(profile)?.sync_all();
    }
    let pending = profile.join(format!(".jailgun-owner-{}.pending", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&pending)?;
    let result = (|| {
        file.write_all(expected.as_bytes())?;
        file.sync_all()?;
        match fs::hard_link(&pending, &marker) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if matches_owner(&marker, expected.as_bytes())? {
                    Ok(())
                } else {
                    Err(conflict())
                }
            }
            Err(error) => Err(error),
        }
    })();
    drop(file);
    let cleanup = fs::remove_file(pending);
    result?;
    cleanup?;
    File::open(profile)?.sync_all()
}

fn matches_owner(marker: &Path, expected: &[u8]) -> io::Result<bool> {
    let metadata = match fs::symlink_metadata(marker) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if !metadata.is_file() || metadata.len() != expected.len() as u64 {
        return Err(conflict());
    }
    let file = File::open(marker)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let opened = file.metadata()?;
        if metadata.dev() != opened.dev() || metadata.ino() != opened.ino() {
            return Err(conflict());
        }
    }
    let mut bytes = Vec::new();
    file.take(expected.len() as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes != expected {
        return Err(conflict());
    }
    Ok(true)
}

fn conflict() -> io::Error {
    io::Error::new(
        io::ErrorKind::AlreadyExists,
        "profile owner is different, linked or invalid",
    )
}

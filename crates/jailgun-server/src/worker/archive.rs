use super::*;

pub(super) fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(super) fn safe_archive_path(path: &Path) -> Result<PathBuf> {
    let mut safe = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) if value != ".git" => safe.push(value),
            Component::CurDir => {}
            _ => {
                return Err(action(
                    "unsafe-archive",
                    "Archive contains an unsafe path.",
                    "Remove absolute paths, parent traversal, links, and .git entries.",
                ))
            }
        }
    }
    if safe.as_os_str().is_empty() {
        return Err(action(
            "unsafe-archive",
            "Archive contains an empty path.",
            "Rebuild the tar.gz with safe relative paths.",
        ));
    }
    Ok(safe)
}

pub(super) fn extract_archive(path: &Path, workspace: &Path) -> Result<()> {
    let file = File::open(path)?;
    let mut archive = Archive::new(GzDecoder::new(file));
    let mut expanded = 0_u64;
    for entry in archive.entries().map_err(Error::Io)? {
        let mut entry = entry.map_err(Error::Io)?;
        let kind = entry.header().entry_type();
        if !kind.is_file() && !kind.is_dir() {
            return Err(action(
                "unsafe-archive",
                "Archives may contain only regular files and directories.",
                "Remove links, devices, and special entries.",
            ));
        }
        let relative = safe_archive_path(&entry.path().map_err(Error::Io)?)?;
        expanded = expanded.saturating_add(entry.header().size().unwrap_or(0));
        if expanded > MAX_OBJECT_BYTES {
            return Err(action(
                "archive-expanded-too-large",
                "Expanded archive exceeds the worker limit.",
                "Reduce the archive before uploading.",
            ));
        }
        let destination = workspace.join(relative);
        if kind.is_dir() {
            std::fs::create_dir_all(destination)?;
        } else {
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut output = File::create(destination)?;
            std::io::copy(&mut entry, &mut output)?;
        }
    }
    Ok(())
}

pub(super) fn build_archive(workspace: &Path, output: &Path) -> Result<()> {
    let file = File::create(output)?;
    let encoder = GzEncoder::new(file, Compression::fast());
    let mut archive = Builder::new(encoder);
    let mut total = 0_u64;
    append_directory(&mut archive, workspace, workspace, &mut total)?;
    archive.finish()?;
    Ok(())
}

pub(super) fn append_directory<W: Write>(
    archive: &mut Builder<W>,
    root: &Path,
    directory: &Path,
    total: &mut u64,
) -> Result<()> {
    let mut entries = std::fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let relative = path.strip_prefix(root).map_err(|_| {
            action(
                "archive-path-failed",
                "Output path escaped its tab workspace.",
                "Inspect the tab workspace and retry.",
            )
        })?;
        if relative
            .components()
            .any(|component| component.as_os_str() == ".git")
        {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            archive.append_dir(relative, &path)?;
            append_directory(archive, root, &path, total)?;
        } else if metadata.is_file() {
            *total = total.saturating_add(metadata.len());
            if *total > MAX_OBJECT_BYTES {
                return Err(action(
                    "output-too-large",
                    "Output files exceed the worker object limit.",
                    "Remove build products or split the result into smaller jobs.",
                ));
            }
            archive.append_path_with_name(&path, relative)?;
        }
    }
    Ok(())
}

pub(super) fn now_ms() -> i64 {
    (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64
}

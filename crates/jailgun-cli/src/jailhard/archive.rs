use super::*;

pub(super) fn create_source_archive(
    invocation_dir: &Path,
    scope: &TargetScope,
    selected: &[SelectedFile],
    archive_path: &Path,
    max_bytes: u64,
) -> Result<SourceManifest> {
    if selected.is_empty() {
        anyhow::bail!("refusing to create an empty source archive");
    }
    let archive_file = File::create(archive_path)
        .with_context(|| format!("creating {}", archive_path.display()))?;
    let encoder = GzEncoder::new(archive_file, Compression::default());
    let mut builder = Builder::new(encoder);
    for file in selected {
        append_regular_file(&mut builder, file)?;
    }
    builder.finish().context("finishing source tar archive")?;
    let mut encoder = builder.into_inner().context("finishing gzip stream")?;
    encoder.try_finish().context("finishing gzip encoder")?;
    drop(encoder);

    let archive_stat =
        fs::metadata(archive_path).with_context(|| format!("stat {}", archive_path.display()))?;
    if archive_stat.len() == 0 {
        anyhow::bail!("archive was not created: {}", archive_path.display());
    }
    if archive_stat.len() > max_bytes {
        anyhow::bail!(
            "source archive is {} bytes, exceeding max-bytes {}",
            archive_stat.len(),
            max_bytes
        );
    }
    let archive_sha256 =
        sha256_file(archive_path).with_context(|| format!("hashing {}", archive_path.display()))?;
    Ok(SourceManifest {
        invocation_dir: invocation_dir.display().to_string(),
        target_paths: scope.display_roots(),
        selected_files: selected
            .iter()
            .map(|file| ManifestFile {
                path: path_to_slash(&file.entry_path),
                size_bytes: file.size_bytes,
            })
            .collect(),
        archive_path: archive_path.display().to_string(),
        archive_sha256,
        archive_size_bytes: archive_stat.len(),
        created_at: timestamp_now(),
    })
}

pub(super) fn append_regular_file(
    builder: &mut Builder<GzEncoder<File>>,
    file: &SelectedFile,
) -> Result<()> {
    let mut input = File::open(&file.abs_path)
        .with_context(|| format!("opening {}", file.abs_path.display()))?;
    let mut header = Header::new_gnu();
    header.set_size(file.size_bytes);
    header.set_mode(0o644);
    header.set_mtime(0);
    header.set_cksum();
    builder
        .append_data(&mut header, &file.entry_path, &mut input)
        .with_context(|| format!("adding {} to archive", file.entry_path.display()))?;
    Ok(())
}

pub(super) fn validate_returned_archive(
    archive_path: &Path,
    invocation_dir: &Path,
    scope: &TargetScope,
    manifest: &SourceManifest,
) -> Result<ReturnedArchiveValidation> {
    if !archive_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.ends_with(".tar.gz"))
        .unwrap_or(false)
    {
        anyhow::bail!(
            "returned artifact must be named with a .tar.gz suffix: {}",
            archive_path.display()
        );
    }
    let stat =
        fs::metadata(archive_path).with_context(|| format!("stat {}", archive_path.display()))?;
    let file =
        File::open(archive_path).with_context(|| format!("opening {}", archive_path.display()))?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    let mut files = Vec::new();
    let mut seen = BTreeSet::new();
    let entries = archive.entries().context("reading returned tar entries")?;
    for entry in entries {
        let entry = entry.context("reading returned tar entry")?;
        let entry_type = entry.header().entry_type();
        if is_metadata_header(entry_type) {
            continue;
        }
        if !entry_type.is_file() {
            anyhow::bail!(
                "returned archive contains a non-file entry: {}",
                entry.path()?.display()
            );
        }
        if entry.header().size().unwrap_or(0) == 0 {
            anyhow::bail!(
                "returned archive contains an empty file: {}",
                entry.path()?.display()
            );
        }
        let path = clean_tar_entry_path(entry.path()?.as_ref())?;
        if !scope.allows(&path) {
            anyhow::bail!(
                "returned archive path is outside selected target scope: {}",
                path.display()
            );
        }
        if !seen.insert(path.clone()) {
            anyhow::bail!(
                "returned archive contains duplicate entry: {}",
                path.display()
            );
        }
        files.push(path_to_slash(&path));
    }
    if files.is_empty() {
        anyhow::bail!("returned archive contains no regular files");
    }
    reject_project_root_folder(&files, invocation_dir, manifest, scope)?;
    Ok(ReturnedArchiveValidation {
        files,
        size_bytes: stat.len(),
    })
}

pub(super) fn reject_project_root_folder(
    files: &[String],
    invocation_dir: &Path,
    manifest: &SourceManifest,
    scope: &TargetScope,
) -> Result<()> {
    let mut first_parts = files
        .iter()
        .filter_map(|file| file.split('/').next())
        .collect::<BTreeSet<_>>();
    if first_parts.len() != 1 {
        return Ok(());
    }
    let Some(shared) = first_parts.pop_first() else {
        return Ok(());
    };
    if files.iter().any(|file| !file.contains('/')) {
        return Ok(());
    }
    let invocation_name = invocation_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let source_top_levels = manifest
        .selected_files
        .iter()
        .filter_map(|file| file.path.split('/').next())
        .collect::<BTreeSet<_>>();
    let target_top_levels = scope
        .display_roots()
        .into_iter()
        .filter(|root| root != ".")
        .filter_map(|root| root.split('/').next().map(str::to_string))
        .collect::<BTreeSet<_>>();
    let selected_top_levels = if target_top_levels.is_empty() {
        source_top_levels
    } else {
        target_top_levels.iter().map(String::as_str).collect()
    };
    if shared == invocation_name || !selected_top_levels.contains(shared) {
        anyhow::bail!(
            "returned archive appears to contain a project root folder ({shared}/); files must be rooted at archive root"
        );
    }
    Ok(())
}

pub(super) fn unpack_validated_archive(
    archive_path: &Path,
    invocation_dir: &Path,
    files: &[String],
) -> Result<()> {
    let allowed = files.iter().cloned().collect::<BTreeSet<_>>();
    let file =
        File::open(archive_path).with_context(|| format!("opening {}", archive_path.display()))?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    for entry in archive.entries().context("reading returned tar entries")? {
        let mut entry = entry.context("reading returned tar entry")?;
        if is_metadata_header(entry.header().entry_type()) {
            continue;
        }
        let path = clean_tar_entry_path(entry.path()?.as_ref())?;
        let display = path_to_slash(&path);
        if !allowed.contains(&display) {
            anyhow::bail!("returned archive changed between validation and unpack: {display}");
        }
        let dest = invocation_dir.join(&path);
        ensure_safe_output_path(invocation_dir, &dest)?;
        entry
            .unpack(&dest)
            .with_context(|| format!("unpacking {}", dest.display()))?;
    }
    Ok(())
}

pub(super) fn ensure_safe_output_path(root: &Path, dest: &Path) -> Result<()> {
    if !path_is_under(dest, root) {
        anyhow::bail!(
            "refusing to write outside invocation directory: {}",
            dest.display()
        );
    }
    if let Ok(meta) = fs::symlink_metadata(dest) {
        if meta.file_type().is_symlink() || !meta.is_file() {
            anyhow::bail!("refusing to overwrite non-regular path: {}", dest.display());
        }
    }
    let parent = dest
        .parent()
        .context("returned archive entry has no parent directory")?;
    let rel_parent = parent.strip_prefix(root).unwrap_or(parent);
    let mut cursor = root.to_path_buf();
    for component in rel_parent.components() {
        let Component::Normal(part) = component else {
            continue;
        };
        cursor.push(part);
        match fs::symlink_metadata(&cursor) {
            Ok(meta) if meta.file_type().is_symlink() => {
                anyhow::bail!(
                    "refusing to write through symlink directory: {}",
                    cursor.display()
                );
            }
            Ok(meta) if !meta.is_dir() => {
                anyhow::bail!(
                    "refusing to write through non-directory: {}",
                    cursor.display()
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&cursor)
                    .with_context(|| format!("creating {}", cursor.display()))?;
            }
            Err(error) => return Err(error).with_context(|| format!("stat {}", cursor.display())),
        }
    }
    Ok(())
}

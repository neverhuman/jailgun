use super::*;

pub(super) fn walk_source_tree(
    invocation_dir: &Path,
    path: &Path,
    scope: &TargetScope,
    canonical_roots: &[PathBuf],
    selected: &mut Vec<SelectedFile>,
) -> Result<()> {
    let symlink_meta =
        fs::symlink_metadata(path).with_context(|| format!("stat {}", path.display()))?;
    if symlink_meta.file_type().is_symlink() {
        let target = fs::canonicalize(path)
            .with_context(|| format!("resolving symlink {}", path.display()))?;
        if !canonical_roots
            .iter()
            .any(|root| path_is_under(&target, root))
        {
            anyhow::bail!("symlink escapes selected target roots: {}", path.display());
        }
        let target_meta = fs::metadata(&target)
            .with_context(|| format!("stat symlink target {}", target.display()))?;
        if target_meta.is_file() {
            let rel = normalize_existing_relative(invocation_dir, path)?;
            if scope.allows(&rel) && include_source_path(&rel) {
                selected.push(SelectedFile {
                    abs_path: target,
                    entry_path: rel.clone(),
                    size_bytes: target_meta.len(),
                });
            }
            return Ok(());
        }
        anyhow::bail!("symlink directories are not supported: {}", path.display());
    }
    if symlink_meta.is_file() {
        let rel = normalize_existing_relative(invocation_dir, path)?;
        if scope.allows(&rel) && include_source_path(&rel) {
            selected.push(selected_file(canonical_roots, path, rel)?);
        }
        return Ok(());
    }
    if !symlink_meta.is_dir() {
        anyhow::bail!("unsupported non-regular source path: {}", path.display());
    }
    let rel = normalize_existing_relative(invocation_dir, path)?;
    if !rel.as_os_str().is_empty() && excluded_path(&rel) {
        return Ok(());
    }
    for entry in fs::read_dir(path).with_context(|| format!("reading {}", path.display()))? {
        let entry = entry?;
        walk_source_tree(
            invocation_dir,
            &entry.path(),
            scope,
            canonical_roots,
            selected,
        )?;
    }
    Ok(())
}

pub(super) fn selected_file(
    canonical_roots: &[PathBuf],
    abs: &Path,
    entry_rel: PathBuf,
) -> Result<SelectedFile> {
    validate_relative_path(&entry_rel)?;
    let symlink_meta =
        fs::symlink_metadata(abs).with_context(|| format!("stat {}", abs.display()))?;
    let mut source_path = abs.to_path_buf();
    let metadata = if symlink_meta.file_type().is_symlink() {
        let target = fs::canonicalize(abs)
            .with_context(|| format!("resolving symlink {}", abs.display()))?;
        if !canonical_roots
            .iter()
            .any(|root| path_is_under(&target, root))
        {
            anyhow::bail!("symlink escapes selected target roots: {}", abs.display());
        }
        source_path = target.clone();
        fs::metadata(&target).with_context(|| format!("stat {}", target.display()))?
    } else {
        fs::metadata(abs).with_context(|| format!("stat {}", abs.display()))?
    };
    if !metadata.is_file() {
        anyhow::bail!("source path is not a regular file: {}", abs.display());
    }
    Ok(SelectedFile {
        abs_path: source_path,
        entry_path: entry_rel.clone(),
        size_bytes: metadata.len(),
    })
}

pub(super) fn canonical_scope_roots(
    invocation_dir: &Path,
    scope: &TargetScope,
) -> Result<Vec<PathBuf>> {
    if scope.all {
        return Ok(vec![fs::canonicalize(invocation_dir).with_context(
            || format!("canonicalizing {}", invocation_dir.display()),
        )?]);
    }
    scope
        .roots
        .iter()
        .map(|root| fs::canonicalize(invocation_dir.join(&root.rel)))
        .collect::<std::io::Result<Vec<_>>>()
        .context("canonicalizing selected target roots")
}

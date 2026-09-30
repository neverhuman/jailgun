use super::*;

impl TargetScope {
    pub(super) fn resolve(invocation_dir: &Path, raw_paths: &[PathBuf]) -> Result<Self> {
        let invocation_canonical = fs::canonicalize(invocation_dir)
            .with_context(|| format!("canonicalizing {}", invocation_dir.display()))?;
        let paths = if raw_paths.is_empty() {
            vec![PathBuf::from(".")]
        } else {
            raw_paths.to_vec()
        };
        let mut roots = Vec::new();
        let mut all = false;
        for raw in paths {
            let abs = absolute_from(invocation_dir, &raw);
            if !abs.exists() {
                anyhow::bail!("target path does not exist: {}", raw.display());
            }
            let canonical = fs::canonicalize(&abs)
                .with_context(|| format!("canonicalizing {}", abs.display()))?;
            if !path_is_under(&canonical, &invocation_canonical) {
                anyhow::bail!(
                    "target path resolves outside the invocation directory: {}",
                    raw.display()
                );
            }
            if !path_is_under(&abs, invocation_dir) {
                anyhow::bail!(
                    "target path must be inside the invocation directory: {}",
                    raw.display()
                );
            }
            let rel = normalize_existing_relative(invocation_dir, &abs)?;
            if rel.as_os_str().is_empty() || rel == Path::new(".") {
                all = true;
            }
            roots.push(ScopeRoot {
                rel,
                is_file: fs::metadata(&abs)
                    .with_context(|| format!("stat {}", abs.display()))?
                    .is_file(),
            });
        }
        Ok(Self { roots, all })
    }

    pub(super) fn allows(&self, path: &Path) -> bool {
        if self.all {
            return true;
        }
        self.roots.iter().any(|root| {
            if root.is_file {
                path == root.rel
            } else {
                path == root.rel || path.starts_with(&root.rel)
            }
        })
    }

    pub(super) fn display_roots(&self) -> Vec<String> {
        if self.all {
            return vec![".".into()];
        }
        self.roots
            .iter()
            .map(|root| path_to_slash(&root.rel))
            .collect()
    }

    pub(super) fn git_pathspecs(
        &self,
        invocation_dir: &Path,
        git_root: &Path,
    ) -> Result<Vec<String>> {
        if self.all {
            let rel = normalize_existing_relative(git_root, invocation_dir)?;
            return Ok(vec![pathspec_or_dot(&rel)]);
        }
        self.roots
            .iter()
            .map(|root| {
                let abs = invocation_dir.join(&root.rel);
                normalize_existing_relative(git_root, &abs).map(|rel| pathspec_or_dot(&rel))
            })
            .collect()
    }
}

pub(super) fn select_source_files(
    invocation_dir: &Path,
    scope: &TargetScope,
) -> Result<Vec<SelectedFile>> {
    if let Some(git_root) = git_root(invocation_dir)? {
        select_git_source_files(invocation_dir, &git_root, scope)
    } else {
        select_recursive_source_files(invocation_dir, scope)
    }
}

/// Parse an `--include-manifest` file into a list of repo-relative paths. Entries are separated by
/// newlines or commas; blank lines and `#`-prefixed comments are ignored. The manifest itself must
/// live under (or be addressable relative to) the invocation directory.
pub(super) fn read_manifest_paths(
    invocation_dir: &Path,
    manifest_path: &Path,
) -> Result<Vec<PathBuf>> {
    let abs = absolute_from(invocation_dir, manifest_path);
    let text = fs::read_to_string(&abs)
        .with_context(|| format!("reading include-manifest {}", abs.display()))?;
    let mut paths = Vec::new();
    // Process line-by-line so a comment line is recognized as a whole BEFORE comma-splitting
    // (otherwise a `#` comment that contains a comma would leak its tail as a fake path).
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        for raw in line.split(',') {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                paths.push(PathBuf::from(trimmed));
            }
        }
    }
    if paths.is_empty() {
        anyhow::bail!("include-manifest {} listed no files", abs.display());
    }
    Ok(paths)
}

/// Select exactly the files named by an `--include-manifest` (each `scope` root is one listed file,
/// already validated to exist under the invocation directory by [`TargetScope::resolve`]). This
/// deliberately **skips the source-extension allowlist** so curated payload files of any extension
/// (`.zyal`, `.cff`, …) are included, but it still enforces the security denylist: no excluded
/// directories (`.git`, `target`, `artifacts`, …), no secret-like filenames (`.env*`, `*.key`, …),
/// no path traversal, and regular files only (via [`selected_file`]).
pub(super) fn select_manifest_source_files(
    invocation_dir: &Path,
    scope: &TargetScope,
) -> Result<Vec<SelectedFile>> {
    let canonical_roots = canonical_scope_roots(invocation_dir, scope)?;
    let mut selected = Vec::new();
    let mut seen = BTreeSet::new();
    for root in &scope.roots {
        let entry_rel = root.rel.clone();
        if entry_rel.as_os_str().is_empty() {
            anyhow::bail!("include-manifest cannot select the repository root");
        }
        if excluded_path(&entry_rel) {
            anyhow::bail!(
                "include-manifest path is inside an excluded directory: {}",
                entry_rel.display()
            );
        }
        if let Some(name) = entry_rel.file_name().and_then(|name| name.to_str()) {
            if secret_like_filename(&name.to_ascii_lowercase()) {
                anyhow::bail!(
                    "include-manifest refuses a secret-like file: {}",
                    entry_rel.display()
                );
            }
        }
        let abs = invocation_dir.join(&entry_rel);
        let file = selected_file(&canonical_roots, &abs, entry_rel)?;
        if seen.insert(file.entry_path.clone()) {
            selected.push(file);
        }
    }
    selected.sort_by(|left, right| left.entry_path.cmp(&right.entry_path));
    if selected.is_empty() {
        anyhow::bail!("include-manifest produced no files");
    }
    Ok(selected)
}

pub(super) fn select_git_source_files(
    invocation_dir: &Path,
    git_root: &Path,
    scope: &TargetScope,
) -> Result<Vec<SelectedFile>> {
    let pathspecs = scope.git_pathspecs(invocation_dir, git_root)?;
    let canonical_roots = canonical_scope_roots(invocation_dir, scope)?;
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(git_root)
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
        ])
        .args(pathspecs);
    let output = command.output().context("running git ls-files")?;
    if !output.status.success() {
        anyhow::bail!(
            "git ls-files failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let mut selected = Vec::new();
    let mut seen = BTreeSet::new();
    for raw in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|raw| !raw.is_empty())
    {
        let root_rel = PathBuf::from(String::from_utf8_lossy(raw).to_string());
        let abs = git_root.join(&root_rel);
        let Ok(entry_rel) = normalize_existing_relative(invocation_dir, &abs) else {
            continue;
        };
        if !scope.allows(&entry_rel) || !include_source_path(&entry_rel) {
            continue;
        }
        let file = selected_file(&canonical_roots, &abs, entry_rel)?;
        if seen.insert(file.entry_path.clone()) {
            selected.push(file);
        }
    }
    selected.sort_by(|left, right| left.entry_path.cmp(&right.entry_path));
    if selected.is_empty() {
        anyhow::bail!("source archive filter produced no code, config, or docs files");
    }
    Ok(selected)
}

pub(super) fn select_recursive_source_files(
    invocation_dir: &Path,
    scope: &TargetScope,
) -> Result<Vec<SelectedFile>> {
    let mut selected = Vec::new();
    let roots = if scope.all {
        vec![PathBuf::new()]
    } else {
        scope.roots.iter().map(|root| root.rel.clone()).collect()
    };
    let canonical_roots = canonical_scope_roots(invocation_dir, scope)?;
    for root in roots {
        walk_source_tree(
            invocation_dir,
            &invocation_dir.join(&root),
            scope,
            &canonical_roots,
            &mut selected,
        )?;
    }
    selected.sort_by(|left, right| left.entry_path.cmp(&right.entry_path));
    selected.dedup_by(|left, right| left.entry_path == right.entry_path);
    if selected.is_empty() {
        anyhow::bail!("source archive filter produced no code, config, or docs files");
    }
    Ok(selected)
}

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

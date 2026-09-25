use super::*;

pub(super) fn include_source_path(path: &Path) -> bool {
    if excluded_path(path) {
        return false;
    }
    let Some(filename) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let lower_name = filename.to_ascii_lowercase();
    if EXCLUDED_FILENAMES.contains(&lower_name.as_str()) || secret_like_filename(&lower_name) {
        return false;
    }
    if CODE_FILENAMES.contains(&lower_name.as_str()) {
        return true;
    }
    if lower_name.starts_with("vite.config.")
        || lower_name.starts_with("vitest.config.")
        || lower_name.starts_with("tsconfig.")
        || lower_name.starts_with("eslint.config.")
        || lower_name.starts_with("prettier.config.")
    {
        return true;
    }
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| SOURCE_EXTENSIONS.contains(&format!(".{}", ext.to_ascii_lowercase()).as_str()))
        .unwrap_or(false)
}

pub(super) fn source_archive_path(work_dir: &Path) -> Result<PathBuf> {
    let path = work_dir.join(SOURCE_ARCHIVE_FILENAME);
    ensure_safe_output_path(work_dir, &path)?;
    Ok(path)
}

pub(super) fn prompt_work_path(work_dir: &Path) -> Result<PathBuf> {
    let path = work_dir.join("hardening-prompt.txt");
    ensure_safe_output_path(work_dir, &path)?;
    Ok(path)
}

pub(super) fn excluded_path(path: &Path) -> bool {
    path.components().any(|component| {
        let Component::Normal(part) = component else {
            return false;
        };
        let part = part.to_string_lossy().to_ascii_lowercase();
        EXCLUDED_DIRS.contains(&part.as_str())
    })
}

pub(super) fn secret_like_filename(lower_name: &str) -> bool {
    lower_name == ".env"
        || lower_name.starts_with(".env.")
        || lower_name.contains("secret")
        || lower_name.contains("token")
        || lower_name.ends_with(".pem")
        || lower_name.ends_with(".key")
        || lower_name.ends_with(".p12")
}

pub(super) fn clean_tar_entry_path(path: &Path) -> Result<PathBuf> {
    validate_relative_path(path)?;
    let mut clean = PathBuf::new();
    for component in path.components() {
        if let Component::Normal(part) = component {
            clean.push(part);
        }
    }
    if clean.as_os_str().is_empty() {
        anyhow::bail!("tar entry path cannot be empty");
    }
    Ok(clean)
}

pub(super) fn validate_relative_path(path: &Path) -> Result<()> {
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                if part.to_string_lossy() == ".git" {
                    anyhow::bail!("path contains forbidden .git component: {}", path.display());
                }
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                anyhow::bail!("unsafe path: {}", path.display());
            }
        }
    }
    Ok(())
}

pub(super) fn normalize_existing_relative(root: &Path, path: &Path) -> Result<PathBuf> {
    let rel = path
        .strip_prefix(root)
        .with_context(|| format!("{} is not under {}", path.display(), root.display()))?;
    validate_relative_path(rel)?;
    Ok(if rel == Path::new(".") {
        PathBuf::new()
    } else {
        rel.to_path_buf()
    })
}

pub(super) fn absolute_from(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

pub(super) fn path_is_under(path: &Path, root: &Path) -> bool {
    path.strip_prefix(root).is_ok()
}

pub(super) fn pathspec_or_dot(path: &Path) -> String {
    if path.as_os_str().is_empty() {
        ".".into()
    } else {
        path_to_slash(path)
    }
}

pub(super) fn path_to_slash(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) fn resolve_config_path(path: &Path) -> PathBuf {
    if path.is_absolute() || path.exists() {
        return path.to_path_buf();
    }
    let workspace_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(path);
    if workspace_path.exists() {
        workspace_path
    } else {
        path.to_path_buf()
    }
}

pub(super) fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub(super) fn timestamp_now() -> String {
    match OffsetDateTime::now_utc().format(&Rfc3339) {
        Ok(timestamp) => timestamp,
        Err(_) => "1970-01-01T00:00:00Z".to_string(),
    }
}

pub(super) fn is_metadata_header(entry_type: EntryType) -> bool {
    entry_type.is_pax_global_extensions()
        || entry_type.is_pax_local_extensions()
        || entry_type.is_gnu_longname()
        || entry_type.is_gnu_longlink()
}

const SOURCE_EXTENSIONS: &[&str] = &[
    ".bash", ".cjs", ".css", ".html", ".js", ".jsx", ".md", ".mdx", ".mjs", ".rs", ".scss", ".sh",
    ".sql", ".toml", ".ts", ".tsx", ".yaml", ".yml", ".json", ".py",
];

const CODE_FILENAMES: &[&str] = &[
    ".dockerignore",
    ".editorconfig",
    ".gitattributes",
    ".gitignore",
    "agants.md",
    "agents.md",
    "cargo.toml",
    "dockerfile",
    "justfile",
    "makefile",
    "package.json",
    "readme.md",
    "rust-toolchain.toml",
    "tsconfig.json",
];

const EXCLUDED_FILENAMES: &[&str] = &[
    "cargo.lock",
    "package-lock.json",
    "pnpm-lock.yaml",
    "poetry.lock",
    "yarn.lock",
];

const EXCLUDED_DIRS: &[&str] = &[
    ".cache",
    ".git",
    ".jailgun",
    ".next",
    ".nuxt",
    ".parcel-cache",
    ".svelte-kit",
    ".turbo",
    ".venv",
    ".vite",
    "artifacts",
    "browser-state",
    "build",
    "coverage",
    "dist",
    "downloads",
    "logs",
    "node_modules",
    "out",
    "profiles",
    "receipts",
    "target",
    "telegram",
    "tmp",
    "vendor",
];

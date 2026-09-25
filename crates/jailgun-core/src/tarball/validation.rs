use std::{
    collections::BTreeSet,
    fs::File,
    path::{Component, Path},
};

use flate2::read::GzDecoder;
use tar::{Archive, EntryType};

use super::{TarError, TarValidation};

pub fn validate_tar_gz(
    path: impl AsRef<Path>,
    require_single_top_level: bool,
) -> Result<TarValidation, TarError> {
    let path = path.as_ref();
    let stat = path.metadata().map_err(|source| TarError::Open {
        path: path.display().to_string(),
        source,
    })?;
    let file = File::open(path).map_err(|source| TarError::Open {
        path: path.display().to_string(),
        source,
    })?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    let mut files = Vec::new();
    let mut top_levels = BTreeSet::new();
    let mut has_child_entry = false;
    let mut entry_count = 0;

    let entries = archive.entries().map_err(|source| TarError::Read {
        path: path.display().to_string(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| TarError::Read {
            path: path.display().to_string(),
            source,
        })?;
        if is_metadata_header(entry.header().entry_type()) {
            continue;
        }
        let entry_path = entry.path().map_err(|source| TarError::Read {
            path: path.display().to_string(),
            source,
        })?;
        let clean = validate_entry_path(&entry_path)?;
        if clean.is_empty() {
            return Err(TarError::UnsafeEntry(entry_path.display().to_string()));
        }
        top_levels.insert(clean[0].clone());
        if clean.len() > 1 {
            has_child_entry = true;
        }
        if entry.header().entry_type().is_file() {
            files.push(clean.join("/"));
        }
        entry_count += 1;
    }

    if entry_count == 0 {
        return Err(TarError::Empty);
    }
    if require_single_top_level && top_levels.len() != 1 {
        return Err(TarError::MultipleTopLevels(
            top_levels.into_iter().collect::<Vec<_>>().join(", "),
        ));
    }
    if require_single_top_level && !has_child_entry {
        return Err(TarError::MissingChildEntry);
    }
    let top_levels = top_levels.into_iter().collect::<Vec<_>>();
    Ok(TarValidation {
        size_bytes: stat.len(),
        entry_count,
        files,
        top_level: (top_levels.len() == 1).then(|| top_levels[0].clone()),
        top_levels,
    })
}

fn validate_entry_path(path: &Path) -> Result<Vec<String>, TarError> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                let value = value.to_string_lossy().to_string();
                if value == ".git" {
                    return Err(TarError::UnsafeEntry(path.display().to_string()));
                }
                parts.push(value);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(TarError::UnsafeEntry(path.display().to_string()));
            }
        }
    }
    Ok(parts)
}

fn is_metadata_header(entry_type: EntryType) -> bool {
    entry_type.is_pax_global_extensions()
        || entry_type.is_pax_local_extensions()
        || entry_type.is_gnu_longname()
        || entry_type.is_gnu_longlink()
}

use std::{
    collections::BTreeSet,
    fs::File,
    path::{Component, Path},
};

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use tar::{Archive, EntryType};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TarError {
    #[error("could not open archive {path}: {source}")]
    Open {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("could not read archive {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("archive contains unsafe entry: {0}")]
    UnsafeEntry(String),
    #[error("archive has no entries")]
    Empty,
    #[error("archive must contain exactly one top-level directory; found: {0}")]
    MultipleTopLevels(String),
    #[error("archive must contain files under its top-level directory")]
    MissingChildEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TarValidation {
    pub size_bytes: u64,
    pub entry_count: usize,
    pub files: Vec<String>,
    pub top_levels: Vec<String>,
    pub top_level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TarCandidate {
    pub index: usize,
    pub text: String,
    pub href: String,
    pub download: String,
    pub aria: String,
    pub title: String,
    pub base_score: i32,
    pub final_score: i32,
}

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

pub fn derive_changed_file_paths(
    validation: &TarValidation,
    strip_components: usize,
) -> Vec<String> {
    validation
        .files
        .iter()
        .filter_map(|file| {
            let parts = file
                .split('/')
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>();
            let stripped = parts
                .iter()
                .skip(strip_components)
                .copied()
                .collect::<Vec<_>>()
                .join("/");
            (!stripped.is_empty()).then_some(stripped)
        })
        .collect()
}

pub fn rank_tar_candidates(candidates: &[TarCandidate], target_name: &str) -> Vec<TarCandidate> {
    let target = normalize_tar_name(target_name);
    let mut ranked = candidates
        .iter()
        .cloned()
        .map(|mut candidate| {
            let mut score = candidate.base_score;
            let haystack = [
                candidate.text.as_str(),
                candidate.href.as_str(),
                candidate.download.as_str(),
                candidate.aria.as_str(),
                candidate.title.as_str(),
            ]
            .join(" ");
            if !target.is_empty() {
                let normalized = normalize_tar_name(&haystack);
                if normalized == target {
                    score += 850;
                } else if normalized.contains(&target) || target.contains(&normalized) {
                    score += 250;
                }
            }
            if haystack.to_ascii_lowercase().contains(".tar.gz") {
                score += 150;
            }
            if haystack.to_ascii_lowercase().contains("download") {
                score += 60;
            }
            candidate.final_score = score;
            candidate
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .final_score
            .cmp(&left.final_score)
            .then(left.index.cmp(&right.index))
    });
    ranked
}

fn normalize_tar_name(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(".tar.gz")
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};
    use std::io::Write;
    use tar::{Builder, EntryType, Header};

    fn write_archive(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).expect("archive file");
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);
        for (name, bytes) in entries {
            let mut header = Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, *name, *bytes)
                .expect("append");
        }
        builder.finish().expect("finish");
        let mut encoder = builder.into_inner().expect("encoder");
        encoder.flush().expect("flush");
        encoder.finish().expect("gzip finish");
    }

    fn write_archive_with_raw_path(path: &Path, raw_path: &[u8], bytes: &[u8]) {
        let file = File::create(path).expect("archive file");
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);
        let mut header = Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.as_mut_bytes()[..raw_path.len()].copy_from_slice(raw_path);
        header.set_cksum();
        builder.append(&header, bytes).expect("append raw path");
        builder.finish().expect("finish");
        let mut encoder = builder.into_inner().expect("encoder");
        encoder.flush().expect("flush");
        encoder.finish().expect("gzip finish");
    }

    fn write_archive_with_pax_global_header(path: &Path) {
        let file = File::create(path).expect("archive file");
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        let pax_bytes = b"24 comment=git archive\n";
        let mut pax_header = Header::new_ustar();
        pax_header.set_entry_type(EntryType::XGlobalHeader);
        pax_header.set_size(pax_bytes.len() as u64);
        pax_header.set_mode(0o644);
        pax_header.set_cksum();
        builder
            .append_data(&mut pax_header, "pax_global_header", &pax_bytes[..])
            .expect("append pax header");

        let bytes = b"ok";
        let mut file_header = Header::new_gnu();
        file_header.set_size(bytes.len() as u64);
        file_header.set_mode(0o644);
        file_header.set_cksum();
        builder
            .append_data(&mut file_header, "root/src/lib.rs", &bytes[..])
            .expect("append file");

        builder.finish().expect("finish");
        let mut encoder = builder.into_inner().expect("encoder");
        encoder.flush().expect("flush");
        encoder.finish().expect("gzip finish");
    }

    #[test]
    fn validates_safe_archive_and_changed_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let archive = temp.path().join("source.tar.gz");
        write_archive(&archive, &[("root/src/lib.rs", b"ok")]);

        let validation = validate_tar_gz(&archive, true).expect("valid");
        assert_eq!(validation.top_level.as_deref(), Some("root"));
        assert_eq!(
            derive_changed_file_paths(&validation, 1),
            vec!["src/lib.rs"]
        );
    }

    #[test]
    fn ignores_git_archive_pax_global_header_for_top_level_validation() {
        let temp = tempfile::tempdir().expect("tempdir");
        let archive = temp.path().join("source.tar.gz");
        write_archive_with_pax_global_header(&archive);

        let validation = validate_tar_gz(&archive, true).expect("valid");
        assert_eq!(validation.top_level.as_deref(), Some("root"));
        assert_eq!(validation.top_levels, vec!["root"]);
        assert_eq!(validation.files, vec!["root/src/lib.rs"]);
    }

    #[test]
    fn rejects_parent_traversal() {
        let temp = tempfile::tempdir().expect("tempdir");
        let archive = temp.path().join("unsafe.tar.gz");
        write_archive_with_raw_path(&archive, b"root/../escape.txt", b"no");

        let error = validate_tar_gz(&archive, false).expect_err("unsafe");
        assert!(error.to_string().contains("unsafe entry"));
    }

    #[test]
    fn ranks_target_archive_first() {
        let candidates = vec![
            TarCandidate {
                index: 0,
                text: "Download notes.md".into(),
                href: "https://example.invalid/notes.md".into(),
                download: String::new(),
                aria: String::new(),
                title: String::new(),
                base_score: 100,
                final_score: 0,
            },
            TarCandidate {
                index: 1,
                text: "Download example-source.tar.gz".into(),
                href: "https://example.invalid/example-source.tar.gz".into(),
                download: "example-source.tar.gz".into(),
                aria: String::new(),
                title: String::new(),
                base_score: 90,
                final_score: 0,
            },
        ];
        let ranked = rank_tar_candidates(&candidates, "example-source.tar.gz");
        assert_eq!(ranked[0].index, 1);
    }
}

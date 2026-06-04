use super::*;
use flate2::{write::GzEncoder, Compression};
use std::{fs::File, io::Write, path::Path};
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

#[test]
fn tar_name_ranking_keeps_rstest_style_invariant() {
    let candidates = vec![
        TarCandidate {
            index: 0,
            text: "source.tar.gz".into(),
            href: String::new(),
            download: String::new(),
            aria: String::new(),
            title: String::new(),
            base_score: 0,
            final_score: 0,
        },
        TarCandidate {
            index: 1,
            text: "source-fixes.tar.gz".into(),
            href: String::new(),
            download: String::new(),
            aria: String::new(),
            title: String::new(),
            base_score: 0,
            final_score: 0,
        },
    ];
    let ranked = rank_tar_candidates(&candidates, "source.tar.gz");
    assert_eq!(ranked[0].index, 0);
}

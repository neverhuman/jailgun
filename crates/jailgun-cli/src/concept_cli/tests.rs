use super::*;
use crate::cli::{Cli, Command};
use clap::Parser;

#[test]
fn loopback_validation_rejects_credential_exfiltration_and_ambiguous_urls() {
    for url in [
        "http://localhost:8787",
        "http://127.0.0.1:1234/",
        "http://[::1]:8787",
    ] {
        assert!(client::loopback_url(url).is_ok());
    }
    for url in [
        "https://localhost",
        "http://evil.invalid",
        "http://localhost.evil.invalid",
        "http://user@localhost",
        "http://localhost/path",
        "http://localhost?token=x",
        "http://localhost#fragment",
    ] {
        assert!(client::loopback_url(url).is_err(), "{url}");
    }
    for id in ["../private", "a?token=x", "a/b", "a#fragment", ""] {
        assert!(operations::run_path(id).is_err());
    }
}

#[test]
fn invalid_concept_files_fail_before_any_daemon_is_started() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/cli-input-proofs");
    std::fs::create_dir_all(&root).unwrap();
    let directory = tempfile::tempdir_in(root).unwrap();
    let path = directory.path().join("concept.md");
    let Command::Brainstorm(options) = Cli::try_parse_from([
        "jailgun",
        "brainstorm",
        "--concept-file",
        path.to_str().unwrap(),
        "--account",
        "synthetic",
    ])
    .unwrap()
    .command
    else {
        panic!("expected brainstorm")
    };
    std::fs::write(&path, [0xff, 0xfe]).unwrap();
    assert_eq!(
        error::response(&operations::request(&options).unwrap_err()).code,
        "concept-invalid-text"
    );
    std::fs::write(&path, vec![b'a'; 256_001]).unwrap();
    assert_eq!(
        error::response(&operations::request(&options).unwrap_err()).code,
        "concept-too-large"
    );
    std::fs::write(&path, "   ").unwrap();
    assert_eq!(
        error::response(&operations::request(&options).unwrap_err()).code,
        "invalid-request"
    );
}

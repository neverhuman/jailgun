use super::*;
use flate2::write::GzEncoder;

#[test]
fn prompt_contains_required_invariants() {
    let manifest = SourceManifest {
        invocation_dir: "/repo".into(),
        target_paths: vec!["crates".into()],
        selected_files: vec![ManifestFile {
            path: "crates/x/src/lib.rs".into(),
            size_bytes: 10,
        }],
        archive_path: "/tmp/source.tar.gz".into(),
        archive_sha256: "a".repeat(64),
        archive_size_bytes: 100,
        created_at: "2026-06-06T00:00:00Z".into(),
    };
    let prompt = hardening_prompt(&manifest, None, None);
    assert!(prompt.contains("aggressive bug, security, performance, refactor, and test"));
    assert!(prompt.contains("Do not make behavior-breaking changes"));
    assert!(prompt.contains("Rust owns durable policy"));
    assert!(prompt.contains("TypeScript owns browser/dashboard surfaces"));
    assert!(prompt.contains("rooted at the archive root"));
}

#[test]
fn target_count_prompt_contains_expander_contract() {
    let manifest = SourceManifest {
        invocation_dir: "/repo".into(),
        target_paths: vec![".".into()],
        selected_files: vec![ManifestFile {
            path: "RollingWindowExpander.py".into(),
            size_bytes: 10,
        }],
        archive_path: "/tmp/source.tar.gz".into(),
        archive_sha256: "a".repeat(64),
        archive_size_bytes: 100,
        created_at: "2026-06-06T00:00:00Z".into(),
    };
    let prompt = hardening_prompt(&manifest, Some(5), None);
    assert!(prompt.contains("source.tar.gz"));
    assert!(prompt.contains("exactly 5 new root-level .py files"));
    assert!(prompt.contains("full source code"));
    assert!(prompt.contains("exactly one BaseExpander subclass"));
    assert!(prompt.contains("STAGE_CLASS"));
    assert!(prompt.contains("Do not use import statements"));
    assert!(prompt.contains("np, pd, time, BaseExpander"));
}

#[test]
fn task_file_prompt_replaces_target_count_task_block() {
    let manifest = SourceManifest {
        invocation_dir: "/repo".into(),
        target_paths: vec!["Generative/shared/stages/encoders".into()],
        selected_files: vec![ManifestFile {
            path: "Generative/shared/stages/encoders/NullEncoder.py".into(),
            size_bytes: 10,
        }],
        archive_path: "/tmp/source.tar.gz".into(),
        archive_sha256: "a".repeat(64),
        archive_size_bytes: 100,
        created_at: "2026-06-06T00:00:00Z".into(),
    };
    let task = "Generate exactly 5 encoder files under Generative/shared/stages/encoders/.\n\
Every file must define exactly one BaseEncoder subclass.\n";
    let prompt = hardening_prompt(&manifest, Some(5), Some(task));
    assert!(prompt.contains(task));
    assert!(prompt.contains("rooted at the archive root"));
    assert!(prompt.contains("Selected target paths: Generative/shared/stages/encoders"));
    assert!(prompt.contains("exactly one BaseEncoder subclass"));
    assert!(!prompt.contains("BaseExpander"));
}

#[test]
fn target_count_arg_rejects_zero() {
    let parsed = JailhardArgs::try_parse_from(["jailhard", "--target-count", "0", "."]);
    assert!(parsed.is_err());
}

#[test]
fn task_file_arg_parses() {
    let parsed = JailhardArgs::try_parse_from([
        "jailhard",
        "--target-count",
        "5",
        "--task-file",
        "/tmp/task.txt",
        ".",
    ])
    .unwrap();
    assert_eq!(parsed.target_count, Some(5));
    assert_eq!(parsed.task_file, Some(PathBuf::from("/tmp/task.txt")));
}

#[test]
fn empty_task_file_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let task_path = temp.path().join("task.txt");
    fs::write(&task_path, "  \n").unwrap();
    let error = read_task_file(&task_path).unwrap_err().to_string();
    assert!(error.contains("task file is empty"));
}

#[test]
fn source_path_filter_excludes_runtime_and_secrets() {
    assert!(include_source_path(Path::new("crates/x/src/lib.rs")));
    assert!(include_source_path(Path::new("apps/web/src/App.tsx")));
    assert!(include_source_path(Path::new("vite.config.ts")));
    assert!(include_source_path(Path::new(
        "expanders/FourierFeatureExpander.py"
    )));
    assert!(!include_source_path(Path::new("target/debug/app")));
    assert!(!include_source_path(Path::new("node_modules/pkg/index.ts")));
    assert!(!include_source_path(Path::new(".env.local")));
    assert!(!include_source_path(Path::new("src/api_token.ts")));
    assert!(!include_source_path(Path::new("Cargo.lock")));
}

#[test]
fn returned_archive_rejects_root_folder() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    let archive_path = temp.path().join("patch.tar.gz");
    write_test_tar(&archive_path, &[("repo/src/lib.rs", b"changed\n")]);
    let scope = TargetScope {
        roots: vec![ScopeRoot {
            rel: PathBuf::new(),
            is_file: false,
        }],
        all: true,
    };
    let manifest = SourceManifest {
        invocation_dir: repo.display().to_string(),
        target_paths: vec![".".into()],
        selected_files: vec![ManifestFile {
            path: "src/lib.rs".into(),
            size_bytes: 10,
        }],
        archive_path: "/tmp/source.tar.gz".into(),
        archive_sha256: "a".repeat(64),
        archive_size_bytes: 100,
        created_at: "2026-06-06T00:00:00Z".into(),
    };
    let error = validate_returned_archive(&archive_path, &repo, &scope, &manifest)
        .expect_err("root folder rejected");
    assert!(error.to_string().contains("project root folder"));
}

#[test]
fn returned_archive_rejects_out_of_scope_file() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("patch.tar.gz");
    write_test_tar(&archive_path, &[("apps/web/src/App.tsx", b"changed\n")]);
    let scope = TargetScope {
        roots: vec![ScopeRoot {
            rel: PathBuf::from("crates"),
            is_file: false,
        }],
        all: false,
    };
    let manifest = SourceManifest {
        invocation_dir: temp.path().display().to_string(),
        target_paths: vec!["crates".into()],
        selected_files: vec![ManifestFile {
            path: "crates/x/src/lib.rs".into(),
            size_bytes: 10,
        }],
        archive_path: "/tmp/source.tar.gz".into(),
        archive_sha256: "a".repeat(64),
        archive_size_bytes: 100,
        created_at: "2026-06-06T00:00:00Z".into(),
    };
    let error = validate_returned_archive(&archive_path, temp.path(), &scope, &manifest)
        .expect_err("out of scope rejected");
    assert!(error.to_string().contains("outside selected target scope"));
}

#[test]
fn local_archive_size_cap_fails() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("src.rs");
    fs::write(&source, "pub fn x() {}\n").unwrap();
    let selected = vec![SelectedFile {
        abs_path: source,
        entry_path: PathBuf::from("src.rs"),
        size_bytes: 14,
    }];
    let scope = TargetScope {
        roots: vec![ScopeRoot {
            rel: PathBuf::new(),
            is_file: false,
        }],
        all: true,
    };
    let error = create_source_archive(
        temp.path(),
        &scope,
        &selected,
        &temp.path().join("source.tar.gz"),
        1,
    )
    .expect_err("oversize archive rejected");
    assert!(error.to_string().contains("exceeding max-bytes"));
}

#[test]
fn git_selection_includes_tracked_and_untracked_source_only() {
    let temp = tempfile::tempdir().unwrap();
    run_git(temp.path(), &["init"]);
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::create_dir_all(temp.path().join("node_modules/pkg")).unwrap();
    fs::write(temp.path().join("src/lib.rs"), "pub fn tracked() {}\n").unwrap();
    fs::write(temp.path().join("src/new.ts"), "export const x = 1;\n").unwrap();
    fs::write(temp.path().join("node_modules/pkg/index.ts"), "ignored\n").unwrap();
    fs::write(temp.path().join(".env.local"), "TOKEN=secret\n").unwrap();
    run_git(temp.path(), &["add", "src/lib.rs"]);

    let scope = TargetScope::resolve(temp.path(), &[]).unwrap();
    let selected = select_source_files(temp.path(), &scope).unwrap();
    let paths = selected
        .iter()
        .map(|file| path_to_slash(&file.entry_path))
        .collect::<Vec<_>>();
    assert!(paths.contains(&"src/lib.rs".to_string()));
    assert!(paths.contains(&"src/new.ts".to_string()));
    assert!(!paths.iter().any(|path| path.contains("node_modules")));
    assert!(!paths.iter().any(|path| path.contains(".env")));
}

#[cfg(unix)]
#[test]
fn recursive_selection_rejects_symlink_escape() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(outside.path().join("secret.rs"), "pub fn secret() {}\n").unwrap();
    symlink(
        outside.path().join("secret.rs"),
        temp.path().join("src/secret.rs"),
    )
    .unwrap();
    let scope = TargetScope::resolve(temp.path(), &[PathBuf::from("src")]).unwrap();
    let error =
        select_recursive_source_files(temp.path(), &scope).expect_err("escaping symlink rejected");
    assert!(error
        .to_string()
        .contains("symlink escapes selected target roots"));
}

#[test]
fn include_manifest_selects_exact_files_and_bypasses_extension_allowlist() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::create_dir_all(temp.path().join("docs")).unwrap();
    fs::write(temp.path().join("src/lib.rs"), "pub fn x() {}\n").unwrap();
    // A `.zyal` file is NOT in the source-extension allowlist, so the default scope walk would drop
    // it — the manifest path must include it verbatim.
    fs::write(temp.path().join("docs/run.zyal"), "<<<ZYAL v1>>>\n").unwrap();
    fs::write(temp.path().join("ignored.rs"), "pub fn nope() {}\n").unwrap();
    fs::write(
        temp.path().join("manifest.txt"),
        "# curated payload\nsrc/lib.rs\ndocs/run.zyal\n",
    )
    .unwrap();

    let paths = read_manifest_paths(temp.path(), Path::new("manifest.txt")).unwrap();
    let scope = TargetScope::resolve(temp.path(), &paths).unwrap();
    let selected = select_manifest_source_files(temp.path(), &scope).unwrap();
    let got = selected
        .iter()
        .map(|file| path_to_slash(&file.entry_path))
        .collect::<Vec<_>>();
    // Exactly the two listed files (sorted), including the .zyal; `ignored.rs` is absent.
    assert_eq!(
        got,
        vec!["docs/run.zyal".to_string(), "src/lib.rs".to_string()]
    );
}

#[test]
fn include_manifest_rejects_secret_like_files() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join(".env.local"), "TOKEN=secret\n").unwrap();
    fs::write(temp.path().join("manifest.txt"), ".env.local\n").unwrap();
    let paths = read_manifest_paths(temp.path(), Path::new("manifest.txt")).unwrap();
    let scope = TargetScope::resolve(temp.path(), &paths).unwrap();
    let error =
        select_manifest_source_files(temp.path(), &scope).expect_err("secret-like file rejected");
    assert!(error.to_string().contains("secret-like"));
}

#[test]
fn include_manifest_rejects_excluded_directories() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("target")).unwrap();
    fs::write(temp.path().join("target/foo.rs"), "pub fn f() {}\n").unwrap();
    fs::write(temp.path().join("manifest.txt"), "target/foo.rs\n").unwrap();
    let paths = read_manifest_paths(temp.path(), Path::new("manifest.txt")).unwrap();
    let scope = TargetScope::resolve(temp.path(), &paths).unwrap();
    let error =
        select_manifest_source_files(temp.path(), &scope).expect_err("excluded directory rejected");
    assert!(error.to_string().contains("excluded directory"));
}

#[test]
fn account_resolution_defaults_to_all_ready_accounts() {
    let temp = tempfile::tempdir().unwrap();
    let registry_path = temp.path().join("browser-profiles.json");
    let roots = jailgun_core::BrowserAccountRoots {
        profile_root: temp.path().join("profiles"),
        state_root: temp.path().join("state"),
        downloads_root: temp.path().join("downloads"),
    };
    let mut registry = BrowserProfileRegistry::default();
    registry
        .upsert_account("one@example.test", Some("acct-one".into()), &roots, 9331, 1)
        .unwrap();
    registry
        .upsert_account("two@example.test", Some("acct-two".into()), &roots, 9332, 1)
        .unwrap();
    registry.accounts[0].status = BrowserAccountStatus::Ready;
    registry.accounts[1].status = BrowserAccountStatus::Locked;
    registry.save(&registry_path).unwrap();

    let env_name = "JAILGUN_TEST_BROWSER_PROFILES_JAILHARD";
    std::env::set_var(env_name, &registry_path);
    let mut config = JailgunConfig::default();
    config.browser.profile_registry_env = env_name.into();
    let accounts = resolve_accounts(&config, &[]).unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id, "acct-one");

    let error = resolve_accounts(&config, &["acct-two".into()])
        .expect_err("explicit non-ready account rejected");
    assert!(error.to_string().contains("not ready"));
    std::env::remove_var(env_name);
}

#[test]
fn high_risk_review_support_count_detects_rejection() {
    let structured = json!({
        "findings": [{
            "severity": "critical",
            "category": "security",
            "supporting_models": ["a", "b"]
        }]
    });
    assert_eq!(high_risk_supporting_models(&structured), 2);
}

fn write_test_tar(path: &Path, files: &[(&str, &[u8])]) {
    let file = File::create(path).unwrap();
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = Builder::new(encoder);
    for (name, bytes) in files {
        let mut header = Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, *name, *bytes)
            .expect("append");
    }
    builder.finish().unwrap();
}

fn run_git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

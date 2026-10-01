use super::{call, initialize_runtime, installed_fixture, parse};
use jailgun_core::installation::Installation;
use serde_json::Value;
use std::path::Path;

fn directory() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/cli-startup-proofs");
    std::fs::create_dir_all(&root).unwrap();
    tempfile::Builder::new()
        .prefix("service $ % \" & ")
        .tempdir_in(root)
        .unwrap()
}

#[tokio::test]
async fn definitions_preserve_stopped_runtime_and_do_not_overwrite_existing_files() {
    let directory = directory();
    let root = directory.path();
    let (binary, assets) = installed_fixture(root);
    let runtime = root.join("private runtime");
    let token = initialize_runtime(&runtime);
    let descriptor = std::fs::read(runtime.join("daemon.json")).unwrap();
    let address: Value = serde_json::from_slice(&descriptor).unwrap();
    for (format, filename) in [("systemd", "jailgun.service"), ("launchd", "jailgun.plist")] {
        let args = ["service", "unit", "--out", filename, "--format", format];
        let output = parse(call(&binary, root, &assets, &args).await);
        assert_eq!(output["status"], "written");
        assert_eq!(output["activated"], false);
        let path = root.join(filename);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains(&token));
        assert!(text.contains(
            address["url"]
                .as_str()
                .unwrap()
                .trim_start_matches("http://")
        ));
        assert!(text.contains("--headless"));
        assert!(!Installation::is_owned(&runtime).unwrap());
        let mut names = std::fs::read_dir(&runtime)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(
            names,
            ["daemon.json", "daemon.lock", "daemon.log", "operator-token"]
        );
        let repeated = call(&binary, root, &assets, &args).await;
        assert!(!repeated.status.success());
        let error: Value = serde_json::from_slice(&repeated.stderr).unwrap();
        assert_eq!(error["code"], "startup-file-exists");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for private in [path, runtime.join("daemon.log")] {
                assert_eq!(
                    std::fs::metadata(private).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
        }
    }
    assert_eq!(
        std::fs::read(runtime.join("daemon.json")).unwrap(),
        descriptor
    );
    assert_eq!(
        std::fs::read_to_string(runtime.join("operator-token"))
            .unwrap()
            .trim(),
        token
    );
}

#[tokio::test]
async fn generation_rejects_live_runtime_nonlocal_options_invalid_paths_and_linked_outputs() {
    let directory = directory();
    let root = directory.path();
    let (binary, assets) = installed_fixture(root);
    let runtime = root.join("private runtime");
    let args = [
        "service",
        "unit",
        "--out",
        "jailgun.service",
        "--format",
        "systemd",
    ];
    let owner = Installation::acquire(&runtime).unwrap();
    let blocked = call(&binary, root, &assets, &args).await;
    assert_eq!(
        serde_json::from_slice::<Value>(&blocked.stderr).unwrap()["code"],
        "runtime-owned"
    );
    assert!(!root.join("jailgun.service").exists());
    drop(owner);
    for (extra, code) in [
        (vec!["--addr", "0.0.0.0:8787"], "loopback-required"),
        (vec!["--url", "http://127.0.0.1:8787"], "startup-local-only"),
        (vec!["--token-file", "secret"], "startup-local-only"),
    ] {
        let output = call(&binary, root, &assets, &[args.as_slice(), &extra].concat()).await;
        assert!(!output.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stderr).unwrap()["code"],
            code
        );
        assert!(!root.join("jailgun.service").exists());
    }
    let invalid = call(
        &binary,
        root,
        &assets,
        &[
            "service",
            "unit",
            "--out",
            "jailgun.plist",
            "--format",
            "systemd",
        ],
    )
    .await;
    assert_eq!(
        serde_json::from_slice::<Value>(&invalid.stderr).unwrap()["code"],
        "startup-output-invalid"
    );
    let missing = call(&binary, root, &root.join("missing assets"), &args).await;
    assert_eq!(
        serde_json::from_slice::<Value>(&missing.stderr).unwrap()["code"],
        "runtime-assets-missing"
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("absent"), root.join("jailgun.service")).unwrap();
        let output = call(&binary, root, &assets, &args).await;
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stderr).unwrap()["code"],
            "startup-file-exists"
        );
        assert!(!root.join("absent").exists());
    }
}

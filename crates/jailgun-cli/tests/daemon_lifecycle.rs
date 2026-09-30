use jailgun_core::installation::{DaemonAddress, Installation};
use rmcp::{transport::TokioChildProcess, ServiceExt};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[path = "daemon_lifecycle/startup.rs"]
mod startup;

struct DaemonGuard {
    runtime: PathBuf,
}
impl Drop for DaemonGuard {
    fn drop(&mut self) {
        let _ = std::process::Command::new(env!("CARGO_BIN_EXE_jailgun"))
            .args(["service", "stop", "--runtime"])
            .arg(&self.runtime)
            .env_clear()
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}
async fn call(binary: &Path, root: &Path, assets: &Path, args: &[&str]) -> std::process::Output {
    let mut command = tokio::process::Command::new(binary);
    command
        .args(args)
        .args(["--json", "--runtime"])
        .arg(root.join("private runtime"))
        .arg("--assets")
        .arg(assets)
        .arg("--node")
        .arg(binary)
        .arg("--headless")
        .env_clear()
        .env("HOME", root)
        .current_dir(root)
        .kill_on_drop(true);
    tokio::time::timeout(Duration::from_secs(25), command.output())
        .await
        .unwrap()
        .unwrap()
}

fn installed_fixture(root: &Path) -> (PathBuf, PathBuf) {
    let prefix = root.join("installed application");
    let assets = prefix.join("lib/jailgun");
    let binary = prefix.join("bin/jailgun");
    std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
    std::fs::hard_link(env!("CARGO_BIN_EXE_jailgun"), &binary).unwrap();
    // No account/browser is created in this lifecycle test. Full runtime assets
    // and real Chrome are exercised separately by the browser proof lane.
    for name in [
        "apps/chrome-bridge/bin/concept-bridge.mjs",
        "apps/dashboard/dist/index.html",
    ] {
        let path = assets.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "synthetic lifecycle fixture").unwrap();
    }
    let node = assets.join("node/bin/node");
    std::fs::create_dir_all(node.parent().unwrap()).unwrap();
    std::fs::hard_link(&binary, node).unwrap();
    (binary, assets)
}

fn initialize_runtime(runtime: &Path) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let installation = Installation::acquire(runtime).unwrap();
    installation
        .record_address(listener.local_addr().unwrap())
        .unwrap();
    let token = installation.operator_token().unwrap();
    let descriptor_path = runtime.join("daemon.json");
    let mut address: DaemonAddress =
        serde_json::from_slice(&std::fs::read(&descriptor_path).unwrap()).unwrap();
    // If starting fails, the guard must not signal the test process.
    address.pid = u32::MAX;
    std::fs::write(descriptor_path, serde_json::to_vec(&address).unwrap()).unwrap();
    token
}
fn parse(output: std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[tokio::test]
async fn local_commands_start_one_daemon_outside_checkout_and_reuse_private_state() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/cli-daemon-proofs");
    std::fs::create_dir_all(&root).unwrap();
    let directory = tempfile::Builder::new()
        .prefix("installation with spaces ")
        .tempdir_in(root)
        .unwrap();
    let root = directory.path();
    let (binary, assets) = installed_fixture(root);
    let runtime = root.join("private runtime");
    let token = initialize_runtime(&runtime);
    let descriptor_path = runtime.join("daemon.json");
    let guard = DaemonGuard {
        runtime: runtime.clone(),
    };
    let (first, second) = tokio::join!(
        call(&binary, root, &assets, &["accounts", "list"]),
        call(&binary, root, &assets, &["accounts", "list"])
    );
    assert_eq!(parse(first), serde_json::json!([]));
    assert_eq!(parse(second), serde_json::json!([]));
    let descriptor = std::fs::read(&descriptor_path).unwrap();
    let first = parse(call(&binary, root, &assets, &["setup", "--no-open"]).await);
    let second = parse(call(&binary, root, &assets, &["setup", "--no-open"]).await);
    assert_ne!(first["dashboard_url"], second["dashboard_url"]);
    assert_eq!(std::fs::read(&descriptor_path).unwrap(), descriptor);
    assert_eq!(
        std::fs::read_to_string(runtime.join("operator-token"))
            .unwrap()
            .trim(),
        token
    );
    assert!(Installation::acquire(&runtime).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&runtime).unwrap().permissions().mode() & 0o777,
            0o700
        );
        for name in ["operator-token", "daemon.json", "daemon.log"] {
            assert_eq!(
                std::fs::metadata(runtime.join(name))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
    drop(guard);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if Installation::acquire(&runtime).is_ok() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "daemon did not release ownership"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn installed_defaults_and_mcp_autostart_work_without_checkout_or_explicit_token() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/cli-installed-proofs");
    std::fs::create_dir_all(&parent).unwrap();
    let directory = tempfile::Builder::new()
        .prefix("installation with spaces ")
        .tempdir_in(parent)
        .unwrap();
    let root = directory.path();
    let (binary, assets) = installed_fixture(root);
    let config = assets.join("config/jailgun.example.toml");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(
        &config,
        include_str!("../../../config/jailgun.example.toml")
            .replace("example-project", "installed-regression"),
    )
    .unwrap();
    let output = tokio::process::Command::new(&binary)
        .arg("validate-config")
        .env_clear()
        .current_dir(root)
        .output()
        .await
        .unwrap();
    assert_eq!(parse(output)["project"]["name"], "installed-regression");
    let output = tokio::process::Command::new(&binary)
        .args(["validate-config", "--config", "missing-explicit.toml"])
        .env_clear()
        .current_dir(root)
        .output()
        .await
        .unwrap();
    assert!(
        !output.status.success(),
        "an explicit missing config must not load a different file"
    );
    let runtime = root.join("private runtime");
    let token = initialize_runtime(&runtime);
    let guard = DaemonGuard {
        runtime: runtime.clone(),
    };
    let mut descriptor = None;
    for _ in 0..2 {
        let mut command = tokio::process::Command::new(&binary);
        command
            .args(["mcp", "--runtime"])
            .arg(&runtime)
            .arg("--headless")
            .env_clear()
            .env("HOME", root)
            .current_dir(root)
            .kill_on_drop(true);
        let transport = TokioChildProcess::new(command).unwrap();
        let client = tokio::time::timeout(
            Duration::from_secs(25),
            rmcp::model::ClientConfig::default().serve(transport),
        )
        .await
        .unwrap()
        .unwrap();
        let tools = client.list_all_tools().await.unwrap();
        assert!(tools.iter().any(|tool| tool.name == "jailgun.brainstorm"));
        assert!(tools.iter().any(|tool| tool.name == "jailgun.run"));
        let next = std::fs::read(runtime.join("daemon.json")).unwrap();
        if let Some(previous) = &descriptor {
            assert_eq!(previous, &next, "MCP started a second daemon");
        }
        descriptor = Some(next);
        assert_eq!(
            std::fs::read_to_string(runtime.join("operator-token"))
                .unwrap()
                .trim(),
            token
        );
        client.cancel().await.unwrap();
    }
    drop(guard);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while Installation::acquire(&runtime).is_err() {
        assert!(std::time::Instant::now() < deadline, "daemon did not stop");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn doctor_reports_missing_assets_without_creating_runtime() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/cli-doctor-proofs");
    std::fs::create_dir_all(&root).unwrap();
    let directory = tempfile::tempdir_in(root).unwrap();
    let root = directory.path();
    let output = call(
        Path::new(env!("CARGO_BIN_EXE_jailgun")),
        root,
        &root.join("absent"),
        &["doctor"],
    )
    .await;
    assert!(!output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "needs-attention");
    assert!(!root.join("private runtime").exists());
}

#[tokio::test]
async fn authenticated_stop_verifies_runtime_releases_locks_and_allows_backup_and_restart() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/cli-service-proofs");
    std::fs::create_dir_all(&parent).unwrap();
    let directory = tempfile::tempdir_in(parent).unwrap();
    let root = directory.path();
    let (binary, assets) = installed_fixture(root);
    let runtime = root.join("private runtime");
    let stopped = parse(call(&binary, root, &assets, &["service", "stop"]).await);
    assert_eq!(stopped["state"], "stopped");
    assert!(
        !runtime.exists(),
        "stopping an absent daemon must not initialize a runtime"
    );
    initialize_runtime(&runtime);
    let _guard = DaemonGuard {
        runtime: runtime.clone(),
    };
    parse(call(&binary, root, &assets, &["accounts", "list"]).await);
    let running = parse(call(&binary, root, &assets, &["service", "status"]).await);
    assert_eq!(running["state"], "running");
    assert_eq!(
        running["runtime"],
        runtime.canonicalize().unwrap().to_str().unwrap()
    );
    let other = root.join("different runtime");
    let ownership = Installation::acquire(&other).unwrap();
    for name in ["operator-token", "daemon.json"] {
        std::fs::copy(runtime.join(name), other.join(name)).unwrap();
    }
    let output = tokio::process::Command::new(&binary)
        .args(["service", "stop", "--runtime"])
        .arg(&other)
        .arg("--json")
        .env_clear()
        .output()
        .await
        .unwrap();
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "service-runtime-mismatch");
    drop(ownership);
    assert!(Installation::is_owned(&runtime).unwrap());
    let stopped = parse(call(&binary, root, &assets, &["service", "stop"]).await);
    assert_eq!(stopped["state"], "stopped");
    assert_eq!(stopped["instance_id"], running["instance_id"]);
    assert!(!Installation::is_owned(&runtime).unwrap());
    let backup = root.join("verified backup");
    let output = tokio::process::Command::new(&binary)
        .args(["data", "backup", "--runtime"])
        .arg(&runtime)
        .arg("--out")
        .arg(&backup)
        .arg("--json")
        .env_clear()
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        parse(call(&binary, root, &assets, &["service", "stop"]).await)["state"],
        "stopped"
    );
    parse(call(&binary, root, &assets, &["accounts", "list"]).await);
    let restarted = parse(call(&binary, root, &assets, &["service", "status"]).await);
    assert_eq!(restarted["state"], "running");
    assert_ne!(restarted["instance_id"], running["instance_id"]);
    let descriptor: DaemonAddress =
        serde_json::from_slice(&std::fs::read(runtime.join("daemon.json")).unwrap()).unwrap();
    let requested = parse(
        call(
            &binary,
            root,
            &assets,
            &["service", "stop", "--url", &descriptor.url],
        )
        .await,
    );
    assert_eq!(
        requested["state"], "stopping",
        "an explicit URL cannot prove remote filesystem ownership was released"
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while Installation::is_owned(&runtime).unwrap() {
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

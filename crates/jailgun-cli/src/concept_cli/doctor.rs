use super::{connect::runtime_root, options::ConnectionOptions};
use anyhow::Result;
use serde_json::{json, Value};
use std::{path::PathBuf, time::Duration};

pub async fn inspect(options: &ConnectionOptions) -> Result<Value> {
    let runtime = runtime_root(options)?;
    let assets = match &options.daemon.assets {
        Some(path) => path.clone(),
        None => jailgun_core::runtime_assets::root()?,
    };
    let node = options
        .daemon
        .node
        .clone()
        .unwrap_or_else(|| assets.join("node/bin/node"));
    let mut checks = Vec::new();
    for (name, path, action) in [
        (
            "browser-bridge",
            assets.join("apps/chrome-bridge/bin/concept-bridge.mjs"),
            "Install a complete bundle or pass --assets.",
        ),
        (
            "dashboard",
            assets.join("apps/dashboard/dist/index.html"),
            "Install a complete bundle or build the dashboard.",
        ),
    ] {
        checks.push(json!({"name":name,"ok":path.is_file(),"path":path,"next_action":action}));
    }
    let node_version = version(&node).await;
    let pinned = include_str!("../../../../.node-version").trim();
    checks.push(json!({"name":"node","ok":node_version.as_deref()==Some(&format!("v{pinned}")),"observed":node_version,"required":pinned,"next_action":"Install the pinned Node runtime from the Jailgun bundle."}));
    let mut chrome = options.daemon.chrome.clone();
    if chrome.is_none() {
        for candidate in [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/usr/bin/google-chrome",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
        ] {
            if std::path::Path::new(candidate).is_file() {
                chrome = Some(candidate.into());
                break;
            }
        }
    }
    let chrome_version = match chrome {
        Some(path) => version(&path).await,
        None => None,
    };
    checks.push(json!({"name":"chrome","ok":chrome_version.is_some(),"observed":chrome_version,"next_action":"Install Google Chrome, or supply --chrome pointing to an installed compatible browser. Keep its sandbox enabled."}));
    if cfg!(target_os = "linux")
        && !options.daemon.headless
        && !options.daemon.server_browser
        && std::env::var_os("DISPLAY").is_none()
    {
        checks.push(json!({"name":"interactive-display","ok":false,"next_action":"Use a desktop session or start with --server-browser for Linux dashboard login. Headless mode requires an already authenticated profile."}));
    }
    if options.daemon.server_browser {
        checks.push(json!({"name":"server-browser-platform","ok":cfg!(target_os="linux"),"next_action":"Use the local Chrome window on macOS; managed server displays require Linux."}));
        for name in ["Xvfb", "x11vnc"] {
            let installed = std::env::var_os("PATH").is_some_and(|paths| {
                std::env::split_paths(&paths).any(|path| executable_file(&path.join(name)))
            });
            checks.push(json!({"name":name,"ok":installed,"next_action":"On Ubuntu, install the display dependencies explicitly: sudo apt-get install xvfb x11vnc. Then rerun jailgun doctor --server-browser."}));
        }
    }
    let ok = checks.iter().all(|c| c["ok"] == true);
    Ok(
        json!({"status":if ok{"ready"}else{"needs-attention"},"version":env!("CARGO_PKG_VERSION"),"runtime":runtime,"assets":assets,"checks":checks,"live_provider_verified":false}),
    )
}
fn executable_file(path: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}
async fn version(path: &PathBuf) -> Option<String> {
    let mut command = tokio::process::Command::new(path);
    command.arg("--version").kill_on_drop(true).env_clear();
    for key in ["PATH", "HOME", "LANG", "TMPDIR"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let output = tokio::time::timeout(Duration::from_secs(5), command.output())
        .await
        .ok()?
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

use super::{client::Client, error::failure};
use anyhow::Result;
use serde_json::{json, Value};
use std::process::Stdio;

pub async fn dashboard(client: &Client, no_open: bool) -> Result<Value> {
    let paired: Value = client.post("/api/session/pairing-code", &json!({})).await?;
    let code = paired["code"].as_str().ok_or_else(|| {
        failure(
            "response-invalid",
            "The daemon did not issue a pairing code.",
            "Check that the daemon supports this CLI version.",
        )
    })?;
    uuid::Uuid::parse_str(code).map_err(|_| {
        failure(
            "response-invalid",
            "The pairing code is malformed.",
            "Inspect the daemon version.",
        )
    })?;
    let mut url = client.url.clone();
    url.set_fragment(Some(&format!("pair={code}&next=accounts")));
    let opened = if no_open {
        false
    } else {
        open_browser(url.as_str()).await
    };
    Ok(
        json!({"status":"onboarding-ready","dashboard_url":url.as_str(),"expires_in_seconds":paired["expires_in_seconds"],"browser_opened":opened}),
    )
}
async fn open_browser(url: &str) -> bool {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let mut command = tokio::process::Command::new(program);
    command
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .env_clear();
    for key in [
        "PATH",
        "HOME",
        "USER",
        "DISPLAY",
        "XAUTHORITY",
        "XDG_RUNTIME_DIR",
        "DBUS_SESSION_BUS_ADDRESS",
        "LANG",
        "TMPDIR",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    matches!(tokio::time::timeout(std::time::Duration::from_secs(5),command.status()).await,Ok(Ok(status)) if status.success())
}

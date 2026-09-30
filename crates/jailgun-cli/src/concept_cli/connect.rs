use super::{
    client::{loopback_url, Client},
    error::failure,
    options::ConnectionOptions,
};
use anyhow::{Context, Result};
use jailgun_core::installation::DaemonAddress;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub fn runtime_root(options: &ConnectionOptions) -> Result<PathBuf> {
    match &options.daemon.runtime {
        Some(root) => Ok(root.clone()),
        None => std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".jailgun"))
            .ok_or_else(|| {
                failure(
                    "runtime-required",
                    "A runtime directory is required.",
                    "Set HOME or pass --runtime.",
                )
            }),
    }
}
fn credential(options: &ConnectionOptions, runtime: &Path) -> Result<Option<String>> {
    let bytes = if let Some(path) = &options.token_file {
        Some(crate::commands::mcp::read_token_file(path)?)
    } else if let Some(token) = std::env::var_os("JAILGUN_TOKEN") {
        Some(
            token
                .into_string()
                .map_err(|_| {
                    failure(
                        "credential-invalid",
                        "The credential is not UTF-8.",
                        "Provide an unrevoked credential.",
                    )
                })?
                .into_bytes(),
        )
    } else {
        let path = runtime.join("operator-token");
        if path.exists() {
            Some(crate::commands::mcp::read_token_file(&path)?)
        } else {
            None
        }
    };
    bytes
        .map(|bytes| {
            let value =
                String::from_utf8(bytes).context("credential-invalid: token must be UTF-8")?;
            let value = value.trim();
            if value.is_empty() || value.chars().any(char::is_whitespace) {
                return Err(failure(
                    "credential-invalid",
                    "Expected one credential.",
                    "Set a valid private token file or JAILGUN_TOKEN.",
                ));
            }
            Ok(value.to_string())
        })
        .transpose()
}
fn address(options: &ConnectionOptions, runtime: &Path) -> Result<reqwest::Url> {
    if let Some(url) = &options.url {
        return loopback_url(url);
    }
    let path = runtime.join("daemon.json");
    if path.exists() {
        let bytes = crate::commands::mcp::read_token_file(&path)?;
        let descriptor: DaemonAddress = serde_json::from_slice(&bytes).map_err(|_| {
            failure(
                "daemon-descriptor-invalid",
                "The local daemon descriptor is invalid.",
                "Run doctor and inspect the private runtime before restarting.",
            )
        })?;
        loopback_url(&descriptor.url)
    } else {
        loopback_url("http://127.0.0.1:8787")
    }
}
pub async fn connect(options: &ConnectionOptions) -> Result<Client> {
    let runtime = runtime_root(options)?;
    let url = address(options, &runtime)?;
    if let Some(token) = credential(options, &runtime)? {
        let client = Client::new(url.clone(), token)?;
        if client.probe().await? {
            return Ok(client);
        }
    }
    if options.no_start || options.url.is_some() {
        return Err(failure("daemon-unavailable","No authenticated daemon is available at this address.","Start jailgun serve for this runtime or verify the SSH forward and explicit credential."));
    }
    jailgun_core::browser_registry::ensure_private_dir(&runtime)?;
    let runtime = runtime.canonicalize()?;
    let mut child = start(options, &runtime, &url)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(token) = credential(options, &runtime)? {
            let client = Client::new(address(options, &runtime)?, token)?;
            if client.probe().await? {
                return Ok(client);
            }
        }
        let exited = child.try_wait()?.is_some();
        if Instant::now() >= deadline {
            return Err(failure(
                "daemon-start-failed",
                if exited {
                    "The daemon exited before becoming ready."
                } else {
                    "The daemon has not become ready."
                },
                format!(
                    "Inspect {} and run jailgun doctor with the same runtime.",
                    runtime.join("daemon.log").display()
                ),
            ));
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}
fn start(
    options: &ConnectionOptions,
    runtime: &Path,
    url: &reqwest::Url,
) -> Result<std::process::Child> {
    let log = runtime.join("daemon.log");
    if std::fs::symlink_metadata(&log).is_ok_and(|metadata| !metadata.is_file()) {
        return Err(failure(
            "runtime-path-invalid",
            "The daemon log is not a regular file.",
            "Inspect the private runtime.",
        ));
    }
    let mut open = std::fs::OpenOptions::new();
    open.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        open.mode(0o600);
    }
    let output = open.open(&log)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        output.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["serve", "--runtime"])
        .arg(runtime)
        .arg("--addr")
        .arg(format!(
            "{}:{}",
            url.host_str().unwrap_or("127.0.0.1"),
            url.port_or_known_default().unwrap_or(8787)
        ))
        .current_dir(runtime)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(output)
        .env_clear();
    for key in [
        "PATH",
        "HOME",
        "USER",
        "DISPLAY",
        "XAUTHORITY",
        "XDG_RUNTIME_DIR",
        "TMPDIR",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    for (flag, path) in [
        ("--assets", &options.daemon.assets),
        ("--node", &options.daemon.node),
        ("--chrome", &options.daemon.chrome),
    ] {
        if let Some(path) = path {
            command.arg(flag).arg(path.canonicalize()?);
        }
    }
    if options.daemon.headless {
        command.arg("--headless");
    }
    if options.daemon.server_browser {
        command.arg("--server-browser");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
        .spawn()
        .context("daemon-start-failed: could not launch the local service")
}

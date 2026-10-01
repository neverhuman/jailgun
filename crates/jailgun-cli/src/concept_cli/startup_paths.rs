use super::{client::loopback_url, error::failure, options::ConnectionOptions};
use anyhow::{Context, Result};
use jailgun_core::installation::DaemonAddress;
use std::{
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
};

pub fn address(runtime: &Path) -> Result<SocketAddr> {
    let descriptor = runtime.join("daemon.json");
    if !descriptor.exists() {
        return Ok(SocketAddr::from(([127, 0, 0, 1], 8787)));
    }
    let bytes = crate::commands::mcp::read_token_file(&descriptor)?;
    let descriptor: DaemonAddress = serde_json::from_slice(&bytes)
        .context("daemon-descriptor-invalid: inspect the saved address")?;
    let url = loopback_url(&descriptor.url)?;
    let host = url.host_str().context("daemon-url-invalid")?;
    let ip = if host == "localhost" {
        IpAddr::from([127, 0, 0, 1])
    } else {
        host.trim_matches(['[', ']']).parse()?
    };
    Ok(SocketAddr::new(
        ip,
        url.port_or_known_default().unwrap_or(8787),
    ))
}

pub fn executable() -> Result<PathBuf> {
    let executable = std::env::current_exe()?.canonicalize()?;
    match jailgun_core::managed_installation::ManagedInstallation::for_executable(&executable)
        .map_err(super::error::installation)?
    {
        Some(installation) => installation
            .active_binary("jailgun")
            .map_err(super::error::installation),
        None => Ok(executable),
    }
}

pub fn check_assets(options: &ConnectionOptions) -> Result<()> {
    let assets = match &options.daemon.assets {
        Some(path) => path.clone(),
        None => jailgun_core::runtime_assets::root()?,
    };
    let node = options
        .daemon
        .node
        .clone()
        .unwrap_or_else(|| assets.join("node/bin/node"));
    if !node.is_file()
        || !assets
            .join("apps/chrome-bridge/bin/concept-bridge.mjs")
            .is_file()
        || !assets.join("apps/dashboard/dist/index.html").is_file()
    {
        return Err(failure("runtime-assets-missing", "Startup requires the installed Node runtime, bridge and dashboard.", "Install a complete bundle, or build the development assets and pass --assets and --node. Then run jailgun doctor."));
    }
    Ok(())
}

use super::{
    connect::runtime_root, error::failure, operations::print_value, options::ConnectionOptions,
};
use anyhow::{Context, Result};
use jailgun_core::{
    installation::Installation,
    startup_unit::{validate_name, Definition},
};
use std::{
    fs::OpenOptions,
    io::Write,
    net::SocketAddr,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum UnitFormat {
    Systemd,
    Launchd,
}

#[derive(Debug, clap::Args)]
pub struct UnitOptions {
    /// New .service or .plist file. The parent directory must already exist.
    #[arg(long)]
    pub out: PathBuf,
    /// Defaults to this host's user service manager.
    #[arg(long)]
    pub format: Option<UnitFormat>,
    /// Local listening address; defaults to the saved daemon address or 127.0.0.1:8787.
    #[arg(long)]
    pub addr: Option<SocketAddr>,
}

pub fn write(options: ConnectionOptions, unit: UnitOptions, json: bool) -> Result<bool> {
    if options.url.is_some() || options.token_file.is_some() {
        return Err(failure(
            "startup-local-only",
            "Startup definitions configure a local runtime and contain no credentials.",
            "Omit --url and --token-file when generating a service definition.",
        ));
    }
    let format = unit.format.unwrap_or(if cfg!(target_os = "macos") {
        UnitFormat::Launchd
    } else {
        UnitFormat::Systemd
    });
    let extension = match format {
        UnitFormat::Systemd => "service",
        UnitFormat::Launchd => "plist",
    };
    let invalid_output = || {
        failure(
            "startup-output-invalid",
            "Select a new named .service or .plist file with an existing parent directory.",
            "Create the destination directory, then use --out with the native extension.",
        )
    };
    let name = unit
        .out
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(invalid_output)?;
    validate_name(name).map_err(|_| invalid_output())?;
    if unit.out.extension().and_then(|value| value.to_str()) != Some(extension) {
        return Err(invalid_output());
    }
    let parent = unit
        .out
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let output = parent
        .canonicalize()
        .map_err(|_| invalid_output())?
        .join(unit.out.file_name().ok_or_else(invalid_output)?);
    let output_text = path_text(&output)?;
    if output_text.chars().any(char::is_control) {
        return Err(invalid_output());
    }
    if std::fs::symlink_metadata(&output).is_ok() {
        return Err(failure(
            "startup-file-exists",
            "The output already exists and was retained.",
            "Inspect it or choose a new output path; service definitions are never overwritten.",
        ));
    }
    let executable = super::startup_paths::executable()?;
    super::startup_paths::check_assets(&options)?;
    let runtime = runtime_root(&options)?;
    let _ownership = Installation::acquire(&runtime).map_err(|error| {
        if !matches!(
            error,
            jailgun_core::browser_registry::BrowserRegistryError::Lock { .. }
        ) {
            return failure(
                "runtime-path-invalid",
                error.to_string(),
                "Inspect the runtime directory and its permissions before configuring startup.",
            );
        }
        failure(
            "runtime-owned",
            "Stop the daemon before configuring automatic startup.",
            "Run jailgun service stop for this runtime, then retry.",
        )
    })?;
    let runtime = runtime.canonicalize()?;
    let address = match unit.addr {
        Some(address) => address,
        None => super::startup_paths::address(&runtime)?,
    };
    if !address.ip().is_loopback() {
        return Err(failure(
            "loopback-required",
            "The service must bind to loopback.",
            "Use an SSH local forward for remote access.",
        ));
    }
    let mut arguments = vec![
        "serve".into(),
        "--runtime".into(),
        path_text(&runtime)?,
        "--addr".into(),
        address.to_string(),
    ];
    for (flag, path) in [
        ("--assets", options.daemon.assets),
        ("--node", options.daemon.node),
        ("--chrome", options.daemon.chrome),
    ] {
        if let Some(path) = path {
            arguments.push(flag.into());
            arguments.push(path_text(&path.canonicalize().context(
                "startup-prerequisite-missing: inspect the explicit runtime paths",
            )?)?);
        }
    }
    if options.daemon.headless {
        arguments.push("--headless".into());
    }
    if options.daemon.server_browser {
        arguments.push("--server-browser".into());
    }
    let definition = Definition {
        name: name.into(),
        executable,
        runtime: runtime.clone(),
        arguments,
    };
    let text = match format {
        UnitFormat::Systemd => definition.systemd(),
        UnitFormat::Launchd => definition.launchd(),
    }
    .map_err(|message| {
        failure(
            "startup-value-invalid",
            message,
            "Use paths without control characters.",
        )
    })?;
    let log = runtime.join("daemon.log");
    if std::fs::symlink_metadata(&log).is_ok_and(|metadata| !metadata.is_file()) {
        return Err(failure(
            "runtime-path-invalid",
            "The daemon log is not a regular file.",
            "Inspect the private runtime before configuring startup.",
        ));
    }
    let mut open = OpenOptions::new();
    open.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        open.mode(0o600);
    }
    let file = open.open(log)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    let mut open = OpenOptions::new();
    open.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        open.mode(0o600);
    }
    let mut file = open
        .open(&output)
        .context("startup-write-failed: output must be a new regular file")?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    std::fs::File::open(output.parent().expect("canonical parent"))?.sync_all()?;
    let path = shell_quote(&output_text);
    let activation = match format {
        UnitFormat::Systemd => vec![
            format!("systemctl --user link -- {path}"),
            "systemctl --user daemon-reload".into(),
            format!(
                "systemctl --user enable --now -- {}",
                shell_quote(
                    output
                        .file_name()
                        .and_then(|value| value.to_str())
                        .expect("validated name")
                )
            ),
        ],
        UnitFormat::Launchd => vec![
            format!("launchctl enable \"gui/$(id -u)/{name}\""),
            format!("launchctl bootstrap \"gui/$(id -u)\" {path}"),
        ],
    };
    print_value(
        &serde_json::json!({"status":"written","activated":false,"path":output,"runtime":runtime,"activation_commands":activation}),
        json,
    )?;
    Ok(true)
}

fn path_text(path: &Path) -> Result<String> {
    path.to_str().map(String::from).ok_or_else(|| {
        failure(
            "startup-path-invalid",
            "Startup paths must be UTF-8.",
            "Choose a UTF-8 installation, runtime and output path.",
        )
    })
}
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

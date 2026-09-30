//! Installed assets are located from the running executable, never the build checkout.
use std::{
    env, io,
    path::{Path, PathBuf},
};

pub const DEFAULT_CONFIG: &str = "config/jailgun.example.toml";

pub fn root() -> io::Result<PathBuf> {
    match env::var_os("JAILGUN_ASSETS") {
        Some(value) if value.is_empty() => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "runtime-assets-invalid: JAILGUN_ASSETS is empty",
        )),
        Some(value) => Ok(PathBuf::from(value)),
        None => root_for_executable(&env::current_exe()?),
    }
}

pub fn root_for_executable(executable: &Path) -> io::Result<PathBuf> {
    executable
        .parent()
        .and_then(Path::parent)
        .map(|prefix| prefix.join("lib/jailgun"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "runtime-assets-missing: executable has no installation prefix",
            )
        })
}

/// Only the shipped default may be resolved in the installation. Explicit
/// configuration paths retain their normal working-directory semantics.
pub fn config_path(requested: &Path) -> PathBuf {
    if requested == Path::new(DEFAULT_CONFIG) && !requested.exists() {
        if let Ok(assets) = root() {
            return assets.join(DEFAULT_CONFIG);
        }
    }
    requested.to_path_buf()
}

pub fn archive_bridge_command() -> io::Result<Vec<String>> {
    let assets = root()?;
    let node = assets.join("node/bin/node");
    let bridge = assets.join("apps/chrome-bridge/bin/chrome-bridge.mjs");
    if !node.is_file() || !bridge.is_file() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "runtime-assets-missing: install a complete Jailgun bundle or set JAILGUN_ASSETS; development may provide --bridge-cmd"));
    }
    [node, bridge]
        .into_iter()
        .map(|path| {
            path.into_os_string().into_string().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "runtime-assets-invalid: browser bridge paths must be UTF-8",
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installation_paths_preserve_spaces_and_do_not_use_working_directory() {
        assert_eq!(
            root_for_executable(Path::new("/opt/Jailgun Release/bin/jailgun")).unwrap(),
            PathBuf::from("/opt/Jailgun Release/lib/jailgun")
        );
        assert_eq!(
            root_for_executable(Path::new("/opt/Jailgun Release/bin/jailhard")).unwrap(),
            PathBuf::from("/opt/Jailgun Release/lib/jailgun")
        );
        assert!(root_for_executable(Path::new("/")).is_err());
    }
    #[test]
    fn explicit_missing_config_is_not_replaced_with_a_shipped_default() {
        for path in [
            "custom.toml",
            "config/private.toml",
            "../config/private.toml",
            "/absent/private.toml",
        ] {
            assert_eq!(config_path(Path::new(path)), PathBuf::from(path));
        }
    }
}

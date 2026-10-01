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
    // macOS may report the public launcher symlink. Assets belong to the
    // resolved release, not the installation's mutable inventory directory.
    let executable = executable.canonicalize()?;
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
    #[cfg(unix)]
    #[test]
    fn managed_launcher_symlinks_resolve_assets_inside_the_verified_release() {
        let directory = tempfile::tempdir().unwrap();
        let prefix = directory
            .path()
            .canonicalize()
            .unwrap()
            .join("installation with spaces $ % \" &");
        let base = prefix.join("lib/jailgun");
        let release = base.join("releases/0.2.0-proof");
        std::fs::create_dir_all(prefix.join("bin")).unwrap();
        std::fs::create_dir_all(release.join("bin")).unwrap();
        std::os::unix::fs::symlink("releases/0.2.0-proof", base.join("current")).unwrap();
        for name in ["jailgun", "jailhard"] {
            let actual = release.join("bin").join(name);
            std::fs::write(&actual, b"synthetic executable").unwrap();
            let launcher = prefix.join("bin").join(name);
            std::os::unix::fs::symlink(format!("../lib/jailgun/current/bin/{name}"), &launcher)
                .unwrap();
            assert_eq!(
                root_for_executable(&launcher).unwrap(),
                release.join("lib/jailgun")
            );
            assert_eq!(
                root_for_executable(&actual).unwrap(),
                release.join("lib/jailgun")
            );
        }
    }
    #[test]
    fn installation_paths_preserve_spaces_and_do_not_use_working_directory() {
        let directory = tempfile::tempdir().unwrap();
        let prefix = directory
            .path()
            .canonicalize()
            .unwrap()
            .join("Jailgun Release");
        std::fs::create_dir_all(prefix.join("bin")).unwrap();
        for name in ["jailgun", "jailhard"] {
            let executable = prefix.join("bin").join(name);
            std::fs::write(&executable, b"synthetic executable").unwrap();
            assert_eq!(
                root_for_executable(&executable).unwrap(),
                prefix.join("lib/jailgun")
            );
        }
        assert!(root_for_executable(Path::new("/")).is_err());
        assert!(root_for_executable(&prefix.join("bin/missing")).is_err());
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

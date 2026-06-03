use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

pub fn infer_github_repo(url: &str) -> Option<String> {
    let value = url.trim();
    if value.is_empty() {
        return None;
    }
    for prefix in [
        "git@github.com:",
        "https://github.com/",
        "http://github.com/",
        "ssh://git@github.com/",
    ] {
        if let Some(rest) = value.strip_prefix(prefix) {
            return owner_repo_from_github_path(rest);
        }
    }
    None
}

pub fn owner_repo_from_github_path(path: &str) -> Option<String> {
    let path = path
        .trim()
        .trim_start_matches('/')
        .trim_end_matches('/')
        .split(['?', '#'])
        .next()?;
    let mut parts = path.split('/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim().trim_end_matches(".git");
    if parts.next().is_some() {
        return None;
    }
    if owner.is_empty() || repo.is_empty() {
        None
    } else {
        Some(format!("{owner}/{repo}"))
    }
}

pub fn bridge_command(args: Vec<String>) -> Result<Vec<String>> {
    if !args.is_empty() {
        return Ok(args);
    }
    match env::var("JAILGUN_BRIDGE_CMD") {
        Ok(value) => {
            let parts = value
                .split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>();
            if parts.is_empty() {
                anyhow::bail!("JAILGUN_BRIDGE_CMD is empty");
            }
            Ok(parts)
        }
        Err(env::VarError::NotPresent) => default_bridge_command(),
        Err(env::VarError::NotUnicode(_)) => {
            anyhow::bail!("JAILGUN_BRIDGE_CMD is not valid UTF-8")
        }
    }
}

fn default_bridge_command() -> Result<Vec<String>> {
    for path in default_bridge_candidates() {
        if path.is_file() {
            return Ok(vec!["node".to_string(), path.display().to_string()]);
        }
    }
    anyhow::bail!(
        "bridge command must be provided with --bridge-cmd or JAILGUN_BRIDGE_CMD; default bridge was not found at apps/chrome-bridge/bin/chrome-bridge.mjs"
    )
}

fn default_bridge_candidates() -> Vec<PathBuf> {
    vec![
        PathBuf::from("apps/chrome-bridge/bin/chrome-bridge.mjs"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/chrome-bridge/bin/chrome-bridge.mjs"),
    ]
}

pub fn arg_or_env(value: Option<String>, env_name: &str, label: &str) -> Result<String> {
    match value {
        Some(value) if !value.trim().is_empty() => Ok(value),
        _ => env::var(env_name)
            .with_context(|| format!("{label} must be provided or set in ${env_name}"))
            .and_then(|value| {
                if value.trim().is_empty() {
                    anyhow::bail!("{label} from ${env_name} is empty");
                }
                Ok(value)
            }),
    }
}

pub fn deploy_remote_command(value: Option<String>, env_name: &str) -> Result<String> {
    if let Some(value) = value {
        return Ok(value);
    }
    match env::var(env_name) {
        Ok(value) => Ok(value),
        Err(env::VarError::NotPresent) if env_name == "JAILGUN_REMOTE_COMMAND" => {
            Ok("bash ci-fast-push.sh".to_string())
        }
        Err(env::VarError::NotPresent) => Ok(String::new()),
        Err(env::VarError::NotUnicode(_)) => {
            anyhow::bail!("remote command environment variable ${env_name} is not valid UTF-8")
        }
    }
}

pub fn path_arg_or_env_or_default(
    value: Option<PathBuf>,
    env_name: &str,
    default: PathBuf,
) -> Result<PathBuf> {
    if let Some(value) = value {
        return Ok(value);
    }
    match env::var(env_name) {
        Ok(value) if !value.trim().is_empty() => Ok(PathBuf::from(value)),
        Ok(_) => anyhow::bail!("path environment variable ${env_name} is empty"),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(env::VarError::NotUnicode(_)) => {
            anyhow::bail!("path environment variable ${env_name} is not valid UTF-8")
        }
    }
}

pub fn profile_pool_env_value(paths: &[PathBuf]) -> Result<String> {
    let joined = env::join_paths(paths).with_context(|| "joining browser profile pool paths")?;
    joined
        .into_string()
        .map_err(|_| anyhow::anyhow!("browser profile pool contains non-UTF-8 paths"))
}

pub fn default_managed_chrome_profile_dir() -> PathBuf {
    home_dir_or_current().join(".google-profile-automation-profile")
}

pub fn default_managed_chrome_state_dir() -> PathBuf {
    home_dir_or_current().join(".google-profile-automation-state")
}

pub fn default_run_id() -> String {
    format!("run-{}", uuid::Uuid::new_v4())
}

pub fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    Ok(())
}

fn home_dir_or_current() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_github_repo_rejects_non_repo_remotes() {
        assert_eq!(infer_github_repo("git@example.com:org/repo.git"), None);
        assert_eq!(
            infer_github_repo("https://notgithub.com/example/repo.git"),
            None
        );
        assert_eq!(
            infer_github_repo("https://github.com/example/repo/tree/main"),
            None
        );
    }
}

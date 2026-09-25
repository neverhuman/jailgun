use std::path::PathBuf;

use anyhow::{Context, Result};
use jailgun_core::{repo_policy, validate_tar_gz, JailgunConfig};

pub(super) async fn validate_config(config: PathBuf) -> Result<()> {
    let config = JailgunConfig::from_toml_path(&config)
        .with_context(|| format!("validating {}", config.display()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&config.redacted_for_display())?
    );
    Ok(())
}

pub(super) async fn tar_validate(archive: PathBuf, require_single_top_level: bool) -> Result<()> {
    let validation = validate_tar_gz(&archive, require_single_top_level)
        .with_context(|| format!("validating {}", archive.display()))?;
    println!("{}", serde_json::to_string_pretty(&validation)?);
    Ok(())
}

pub(super) async fn scan(paths: Vec<PathBuf>) -> Result<()> {
    let mut findings = Vec::new();
    for path in paths {
        if path.is_file() {
            findings.extend(repo_policy::scan_file(&path)?);
        }
    }
    if !findings.is_empty() {
        println!("{}", serde_json::to_string_pretty(&findings)?);
        anyhow::bail!("personal string scan found {} issue(s)", findings.len());
    }
    println!("[]");
    Ok(())
}

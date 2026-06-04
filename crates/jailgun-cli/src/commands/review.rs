use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use jailgun_orchestrator::support::ensure_parent_dir;

use crate::agent;

pub(super) async fn review_packet(
    summary_json: PathBuf,
    base: String,
    head: String,
    repo: PathBuf,
    output: Option<PathBuf>,
    patch_bytes: usize,
) -> Result<()> {
    let packet = agent::build_review_packet(&summary_json, &repo, &base, &head, patch_bytes)?;
    let bytes = serde_json::to_vec_pretty(&packet)?;
    if let Some(output) = output {
        ensure_parent_dir(&output)?;
        fs::write(&output, bytes).with_context(|| format!("writing {}", output.display()))?;
    } else {
        println!("{}", String::from_utf8_lossy(&bytes));
    }
    Ok(())
}

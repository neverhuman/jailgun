use super::{client::Client, error::failure};
use anyhow::Result;
use jailgun_workflow::model::{Artifact, Run};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{io::Write, path::Path};

pub async fn verified(client: &Client, run_id: &str, artifact: &Artifact) -> Result<Vec<u8>> {
    super::operations::run_path(run_id)?;
    if artifact.run_id != run_id {
        return Err(failure(
            "artifact-ownership",
            "The artifact belongs to another run.",
            "Inspect the registered result.",
        ));
    }
    uuid::Uuid::parse_str(&artifact.id).map_err(|_| {
        failure(
            "artifact-invalid",
            "The artifact ID is invalid.",
            "Inspect the registered result.",
        )
    })?;
    let bytes = client.artifact(&artifact.run_id, &artifact.id).await?;
    if bytes.len() as u64 != artifact.byte_length
        || format!("{:x}", Sha256::digest(&bytes)) != artifact.sha256
    {
        return Err(failure(
            "artifact-integrity",
            "The downloaded artifact does not match its registered hash and size.",
            "Retain the server data and inspect or restore a verified backup.",
        ));
    }
    Ok(bytes)
}
pub async fn export(client: &Client, run: Run, out: &Path) -> Result<Value> {
    if std::fs::symlink_metadata(out).is_ok()
        && (std::fs::symlink_metadata(out)?.file_type().is_symlink()
            || !out.is_dir()
            || std::fs::read_dir(out)?.next().is_some())
    {
        return Err(failure(
            "export-directory-not-empty",
            "The export destination must be a new or empty directory.",
            "Choose another --out directory; existing files are never overwritten.",
        ));
    }
    jailgun_core::browser_registry::ensure_private_dir(out)?;
    let claim_path = out.join(".export-in-progress");
    let mut claim = std::fs::OpenOptions::new();
    claim.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        claim.mode(0o600);
    }
    let claim_file = claim.open(&claim_path)?;
    let mut files = Vec::new();
    for artifact in &run.artifacts {
        let bytes = verified(client, &run.id, artifact).await?;
        if artifact.name.is_empty()
            || artifact
                .name
                .chars()
                .any(|c| !c.is_ascii_alphanumeric() && !"-_.".contains(c))
            || [".", ".."].contains(&artifact.name.as_str())
        {
            return Err(failure(
                "artifact-invalid",
                "The registered artifact name is unsafe.",
                "Inspect its metadata before exporting.",
            ));
        }
        let relative = format!("artifacts/{}/{}", artifact.id, artifact.name);
        let path = out.join(&relative);
        jailgun_core::browser_registry::ensure_private_dir(
            path.parent().expect("artifact parent"),
        )?;
        write_new(&path, &bytes)?;
        files.push(json!({"artifact_id":artifact.id,"path":relative,"sha256":artifact.sha256,"byte_length":artifact.byte_length,"completion":artifact.completion}));
        if run.status == "completed"
            && artifact.name == "final.md"
            && artifact.completion == "complete"
        {
            write_new(&out.join("final.md"), &bytes)?;
        }
    }
    let manifest = json!({"schema_version":1,"run":run,"files":files});
    write_new(
        &out.join("export.json"),
        &serde_json::to_vec_pretty(&manifest)?,
    )?;
    drop(claim_file);
    std::fs::remove_file(claim_path)?;
    std::fs::File::open(out)?.sync_all()?;
    Ok(
        json!({"run_id":manifest["run"]["id"],"status":manifest["run"]["status"],"out":out,"artifact_count":files.len(),"manifest":out.join("export.json")}),
    )
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().expect("export file parent");
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(path)?;
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}

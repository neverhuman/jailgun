use crate::concept_cli::error::failure;
use anyhow::Result;
use jailgun_core::{browser_registry::BrowserRegistryError, installation::Installation};
use jailgun_workflow::Store;
use std::path::{Path, PathBuf};

#[derive(Debug, clap::Subcommand)]
pub enum DataCommand {
    /// Back up a stopped runtime using a binary with the matching schema.
    Backup {
        #[arg(long, env = "JAILGUN_RUNTIME")]
        runtime: PathBuf,
        /// New directory under an existing private parent.
        #[arg(long)]
        out: PathBuf,
    },
    /// Restore into a new runtime without relocating browser profiles.
    Restore {
        #[arg(long)]
        backup: PathBuf,
        /// New runtime directory; existing directories are never overwritten.
        #[arg(long)]
        out: PathBuf,
    },
}

pub async fn run(command: DataCommand, json: bool) -> Result<bool> {
    let (operation, out) = match command {
        DataCommand::Backup { runtime, out } => {
            validate_runtime(&runtime)?;
            let _owner = Installation::acquire(&runtime).map_err(|error| match error {
                BrowserRegistryError::Lock { .. } => failure(
                    "runtime-owned",
                    "A daemon owns this runtime; no backup was started.",
                    "Stop the owning daemon, then retry the offline backup.",
                ),
                cause => cause.into(),
            })?;
            let store = Store::open_for_backup(&runtime)?;
            let result = store.backup(out.clone()).await;
            let closed = store.close().await;
            result?;
            closed?;
            ("backup", out)
        }
        DataCommand::Restore { backup, out } => {
            let store = Store::restore(&backup, &out)?;
            store.close().await?;
            ("restore", out)
        }
    };
    let path = out.canonicalize()?;
    if json {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "operation":operation, "status":"completed", "path":path,
                "profiles_included":false, "operator_credential_included":false,
                "profile_references_relocated":false, "automation_token_state_included":true
            }))?
        );
    } else {
        println!("{operation} completed: {}", path.display());
        println!("Browser profiles and the operator credential are not included. Profile references remain at their original paths; automation token state is included.");
        if operation == "restore" {
            println!("Keep the original daemon stopped. Start setup with --runtime pointing to this restored directory when ready.");
        }
    }
    Ok(true)
}

fn validate_runtime(root: &Path) -> Result<()> {
    if !std::fs::symlink_metadata(root).is_ok_and(|m| m.is_dir())
        || !std::fs::symlink_metadata(root.join("workflows.sqlite3")).is_ok_and(|m| m.is_file())
    {
        return Err(failure(
            "runtime-not-found",
            "The runtime must be an existing directory containing a regular workflow database.",
            "Pass the existing runtime root with --runtime; symbolic links are not accepted.",
        ));
    }
    Ok(())
}

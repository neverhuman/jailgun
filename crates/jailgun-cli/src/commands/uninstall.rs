use crate::concept_cli::error::{failure, installation};
use jailgun_core::managed_installation::{self, ManagedInstallation};
use std::path::PathBuf;

pub fn run(prefix: Option<PathBuf>, json: bool) -> anyhow::Result<bool> {
    let prefix = match prefix {
        Some(prefix) => prefix.canonicalize()?,
        None => ManagedInstallation::for_executable(&std::env::current_exe()?)
            .map_err(installation)?
            .ok_or_else(|| failure("installation-not-managed", "This executable is not in a managed installation.", "For a managed installation, pass --prefix. For a manual archive, remove only its extracted directory after stopping its processes."))?
            .prefix,
    };
    managed_installation::uninstall(&prefix).map_err(installation)?;
    if json {
        println!(
            "{}",
            serde_json::json!({"operation":"uninstall","status":"completed","prefix":prefix,"runtime_data":"unchanged"})
        );
    } else {
        println!(
            "Application files removed from {}. Accounts and results are retained.",
            prefix.display()
        );
    }
    Ok(true)
}

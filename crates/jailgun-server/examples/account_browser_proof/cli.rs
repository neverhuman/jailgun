use anyhow::{Context, Result};
use jailgun_workflow::model::{ConceptRequest, Run};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub struct CliClient {
    binary: PathBuf,
    directory: PathBuf,
    url: String,
    token: String,
}
impl CliClient {
    pub fn new(url: &str, token: &str, output: &Path) -> Result<Self> {
        Ok(Self {
            binary: std::env::current_exe()?
                .parent()
                .and_then(|p| p.parent())
                .context("example location")?
                .join("jailgun"),
            directory: output.into(),
            url: url.into(),
            token: token.into(),
        })
    }
    async fn command(&self, args: Vec<String>, json: bool) -> Result<Vec<u8>> {
        let mut command = tokio::process::Command::new(&self.binary);
        command
            .args(args)
            .env_clear()
            .env("JAILGUN_URL", &self.url)
            .env("JAILGUN_TOKEN", &self.token)
            .env("HOME", &self.directory)
            .current_dir(&self.directory)
            .kill_on_drop(true);
        if json {
            command.arg("--json");
        }
        let output = tokio::time::timeout(Duration::from_secs(15), command.output()).await??;
        anyhow::ensure!(
            output.status.success(),
            "CLI operation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output.stdout)
    }
    fn submission(request: &ConceptRequest) -> Vec<String> {
        let mut args = vec![
            "brainstorm".into(),
            request.concept.clone(),
            "--account".into(),
            request.account_id.clone(),
            "--tabs".into(),
            request.candidate_count.to_string(),
            "--constraints".into(),
            request.constraints.clone(),
            "--idempotency-key".into(),
            request.idempotency_key.clone(),
        ];
        for criterion in &request.criteria {
            args.extend([
                "--criterion".into(),
                format!("{}={}", criterion.name, criterion.weight),
            ]);
        }
        args
    }
    pub async fn invoke(&self, name: &str, args: Value) -> Result<Value> {
        let arguments = match name {
            "jailgun.brainstorm" => {
                Self::submission(&serde_json::from_value::<ConceptRequest>(args)?)
            }
            "jailgun.run_status" | "jailgun.run_result" => vec![
                "runs".into(),
                if name == "jailgun.run_status" {
                    "show"
                } else {
                    "result"
                }
                .into(),
                args["run_id"].as_str().context("run ID")?.into(),
            ],
            _ => anyhow::bail!("unsupported CLI proof operation"),
        };
        Ok(serde_json::from_slice(
            &self.command(arguments, true).await?,
        )?)
    }
    pub async fn verify_results(&self, run: &Run, final_bytes: &[u8]) -> Result<()> {
        let plain = self
            .command(vec!["runs".into(), "result".into(), run.id.clone()], false)
            .await?;
        anyhow::ensure!(
            String::from_utf8_lossy(&plain).trim() == String::from_utf8_lossy(final_bytes).trim(),
            "CLI Markdown result mismatch"
        );
        let mut wait = Self::submission(&run.request);
        wait.push("--wait".into());
        let result: Value = serde_json::from_slice(&self.command(wait, true).await?)?;
        anyhow::ensure!(
            result["status"] == "completed" && result["run_id"] == run.id,
            "CLI wait did not return the completed result"
        );
        let destination = self.directory.join("CLI export with spaces");
        self.command(
            vec![
                "runs".into(),
                "export".into(),
                run.id.clone(),
                "--out".into(),
                destination.to_string_lossy().into(),
            ],
            true,
        )
        .await?;
        anyhow::ensure!(
            std::fs::read(destination.join("final.md"))? == final_bytes,
            "CLI exported final mismatch"
        );
        let manifest: Value =
            serde_json::from_slice(&std::fs::read(destination.join("export.json"))?)?;
        anyhow::ensure!(
            manifest["run"]["status"] == "completed"
                && manifest["files"].as_array().context("export files")?.len()
                    == run.artifacts.len(),
            "CLI export manifest mismatch"
        );
        Ok(())
    }
}

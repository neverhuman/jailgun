use anyhow::{Context, Result};
use rmcp::{
    model::{CallToolRequestParams, ClientConfig, ProtocolVersion},
    service::RunningService,
    transport::{
        streamable_http_client::StreamableHttpClientTransportConfig, StreamableHttpClientTransport,
        TokioChildProcess,
    },
    RoleClient, ServiceExt,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use std::{path::Path, time::Duration};

pub struct WorkflowClient {
    http: reqwest::Client,
    url: String,
    secret: String,
    mcp: Option<RunningService<RoleClient, ClientConfig>>,
    cli: Option<super::cli::CliClient>,
}
impl WorkflowClient {
    pub async fn connect(url: &str, mode: &str, output: &Path) -> Result<Self> {
        let http = reqwest::Client::new();
        let issued: Value = http
            .post(format!("{url}/api/tokens"))
            .bearer_auth(super::TOKEN)
            .json(&json!({"name":"Synthetic workflow proof","account_ids":["synthetic"]}))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let secret = issued["secret"]
            .as_str()
            .context("missing automation secret")?
            .to_string();
        let config = ClientConfig::default().with_protocol_version(ProtocolVersion::V_2025_11_25);
        let mcp = match mode {
            "http" | "cli" => None,
            "mcp-http" => {
                let mut transport =
                    StreamableHttpClientTransportConfig::with_uri(format!("{url}/mcp"))
                        .auth_header(&secret);
                transport.allow_stateless = true;
                Some(
                    tokio::time::timeout(
                        Duration::from_secs(10),
                        config.serve(StreamableHttpClientTransport::from_config(transport)),
                    )
                    .await??,
                )
            }
            "mcp-stdio" => {
                let binary = std::env::current_exe()?
                    .parent()
                    .and_then(|p| p.parent())
                    .context("example binary location")?
                    .join("jailgun");
                let mut command = tokio::process::Command::new(binary);
                command
                    .args(["mcp", "--url", url])
                    .env("JAILGUN_TOKEN", &secret)
                    .env_remove("JAILGUN_TOKEN_FILE")
                    .env("RUST_LOG", "info")
                    .current_dir(output);
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let stderr = options.open(output.join("stdio-stderr.log"))?;
                let (transport, _) = TokioChildProcess::builder(command).stderr(stderr).spawn()?;
                Some(
                    tokio::time::timeout(Duration::from_secs(10), config.serve(transport))
                        .await??,
                )
            }
            _ => anyhow::bail!("unknown proof transport"),
        };
        if let Some(client) = &mcp {
            anyhow::ensure!(
                client
                    .peer_info()
                    .context("missing peer info")?
                    .protocol_version
                    == ProtocolVersion::V_2025_11_25,
                "MCP version mismatch"
            );
            let tools = client.list_all_tools().await?;
            anyhow::ensure!(
                tools.len() == 9 && tools.iter().all(|t| t.output_schema.is_some()),
                "MCP schema or permission mismatch"
            );
        }
        let cli = if mode == "cli" {
            Some(super::cli::CliClient::new(url, &secret, output)?)
        } else {
            None
        };
        Ok(Self {
            http,
            url: url.into(),
            secret,
            mcp,
            cli,
        })
    }
    async fn invoke<T: DeserializeOwned>(
        &self,
        name: &'static str,
        args: Value,
        method: reqwest::Method,
        path: &str,
    ) -> Result<T> {
        if let Some(cli) = &self.cli {
            Ok(serde_json::from_value(cli.invoke(name, args).await?)?)
        } else if let Some(mcp) = &self.mcp {
            let result = tokio::time::timeout(
                Duration::from_secs(10),
                mcp.call_tool(
                    CallToolRequestParams::new(name)
                        .with_arguments(args.as_object().context("object args")?.clone()),
                ),
            )
            .await??;
            anyhow::ensure!(
                result.is_error != Some(true),
                "MCP operation failed: {result:?}"
            );
            Ok(serde_json::from_value(
                result
                    .structured_content
                    .context("missing structured output")?,
            )?)
        } else {
            Ok(self
                .http
                .request(method, format!("{}{path}", self.url))
                .bearer_auth(&self.secret)
                .json(&args)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?)
        }
    }
    pub async fn submit(&self, request: impl Serialize) -> Result<Value> {
        self.invoke(
            "jailgun.brainstorm",
            serde_json::to_value(request)?,
            reqwest::Method::POST,
            "/api/concept-runs",
        )
        .await
    }
    pub async fn status(&self, id: &str) -> Result<jailgun_workflow::model::Run> {
        self.invoke(
            "jailgun.run_status",
            json!({"run_id":id}),
            reqwest::Method::GET,
            &format!("/api/runs/{id}"),
        )
        .await
    }
    pub async fn result(&self, id: &str) -> Result<jailgun_workflow::model::FinalResult> {
        self.invoke(
            "jailgun.run_result",
            json!({"run_id":id}),
            reqwest::Method::GET,
            &format!("/api/runs/{id}/result"),
        )
        .await
    }
    pub async fn verify_results(
        &self,
        run: &jailgun_workflow::model::Run,
        final_bytes: &[u8],
    ) -> Result<()> {
        if let Some(cli) = &self.cli {
            cli.verify_results(run, final_bytes).await?;
        }
        if self.mcp.is_some() {
            let manifest = self.result(&run.id).await?;
            let artifact = manifest
                .final_artifact
                .context("missing final artifact reference")?;
            let mut offset = 0;
            let mut bytes = Vec::new();
            loop {
                let chunk: jailgun_workflow::model::ArtifactChunk = self.invoke(
                    "jailgun.run_artifact",
                    json!({"run_id":run.id,"artifact_id":artifact.id,"offset":offset,"limit":64}),
                    reqwest::Method::GET, "unused",
                ).await?;
                anyhow::ensure!(
                    chunk.offset == offset
                        && chunk.text.len() <= 64
                        && chunk.artifact.sha256 == artifact.sha256,
                    "invalid MCP artifact chunk"
                );
                bytes.extend_from_slice(chunk.text.as_bytes());
                match chunk.next_offset {
                    Some(next) => {
                        anyhow::ensure!(!chunk.eof && next > offset, "invalid continuation");
                        offset = next;
                    }
                    None => {
                        anyhow::ensure!(chunk.eof, "missing end of artifact");
                        break;
                    }
                }
            }
            use sha2::{Digest, Sha256};
            anyhow::ensure!(
                bytes == final_bytes
                    && bytes.len() as u64 == artifact.byte_length
                    && format!("{:x}", Sha256::digest(&bytes)) == artifact.sha256,
                "MCP final artifact mismatch"
            );
        }
        Ok(())
    }
    pub async fn close(self) -> Result<()> {
        if let Some(mcp) = self.mcp {
            mcp.cancel().await?;
        }
        Ok(())
    }
}

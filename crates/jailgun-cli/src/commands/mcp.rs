//! A stdio MCP gateway; the authenticated daemon remains the application owner.
use anyhow::{Context, Result};
use rmcp::{
    model::*,
    service::{Peer, RequestContext, ServiceError},
    ErrorData, RoleClient, RoleServer, ServerHandler, ServiceExt,
};
use std::time::Duration;

pub type McpOptions = crate::concept_cli::options::ConnectionOptions;

struct Gateway {
    peer: Peer<RoleClient>,
    info: ServerConfig,
}

fn upstream_error(error: ServiceError) -> ErrorData {
    match error {
        ServiceError::McpError(error) => error,
        _ => ErrorData::internal_error(
            "daemon-unavailable: verify the daemon address and unrevoked token, then reconnect",
            None,
        ),
    }
}

impl ServerHandler for Gateway {
    fn supported_protocol_versions(&self) -> std::borrow::Cow<'static, [ProtocolVersion]> {
        std::borrow::Cow::Borrowed(ProtocolVersion::known_up_to(&ProtocolVersion::V_2025_11_25))
    }
    fn get_info(&self) -> ServerConfig {
        self.info.clone()
    }
    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        self.peer.list_tools(request).await.map_err(upstream_error)
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        self.peer
            .call_tool_once(request)
            .await
            .map_err(upstream_error)
    }
}

pub async fn run(options: McpOptions) -> Result<()> {
    let client = if options.no_start || options.url.is_some() {
        crate::concept_cli::configured_client(&options)?
    } else {
        crate::concept_cli::client::Client::connect(&options).await?
    };
    let transport = client.mcp_transport()?;
    let upstream = tokio::time::timeout(
        Duration::from_secs(10),
        ClientConfig::default()
            .with_protocol_version(ProtocolVersion::V_2025_11_25)
            .serve(transport),
    )
    .await
    .context("daemon-timeout: start jailgun serve and verify its address")?
    .context("daemon-unavailable: verify the daemon address and unrevoked token")?;
    let remote = upstream
        .peer_info()
        .context("daemon-protocol-error: initialization response missing")?;
    let mut info = ServerConfig::new(remote.capabilities.clone())
        .with_protocol_version(remote.protocol_version.clone())
        .with_server_info(Implementation::new("jailgun", env!("CARGO_PKG_VERSION")));
    info.instructions = remote.instructions.clone();
    let gateway = Gateway {
        peer: upstream.peer().clone(),
        info,
    };
    let service = gateway.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    let _ = upstream.cancel().await;
    Ok(())
}

pub(crate) fn read_token_file(path: &std::path::Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    anyhow::ensure!(
        metadata.is_file(),
        "credential-invalid: expected a regular token file"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        anyhow::ensure!(
            metadata.permissions().mode() & 0o077 == 0,
            "credential-permissions: restrict token file permissions to 0600"
        );
    }
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() <= 4096,
        "credential-invalid: token file is too large"
    );
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn credentials_are_never_sent_to_non_loopback_or_ambiguous_urls() {
        for url in [
            "https://127.0.0.1",
            "http://example.invalid",
            "http://user@127.0.0.1",
            "http://127.0.0.1/path",
            "http://127.0.0.1/?token=secret",
            "http://127.0.0.1/#fragment",
        ] {
            let error = run(McpOptions {
                url: Some(url.into()),
                no_start: true,
                ..Default::default()
            })
            .await
            .unwrap_err();
            assert!(
                error.to_string().starts_with("daemon-url-invalid:"),
                "{error}"
            );
        }
    }
    #[test]
    fn credential_files_are_bounded_and_private() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/mcp-credential-proofs");
        std::fs::create_dir_all(&root).unwrap();
        let directory = tempfile::Builder::new().tempdir_in(root).unwrap();
        let file = directory.path().join("token file");
        std::fs::write(&file, "synthetic-value").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(read_token_file(&file)
                .unwrap_err()
                .to_string()
                .starts_with("credential-permissions:"));
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert_eq!(read_token_file(&file).unwrap(), b"synthetic-value");
        std::fs::write(&file, vec![b'x'; 4097]).unwrap();
        assert!(read_token_file(&file)
            .unwrap_err()
            .to_string()
            .starts_with("credential-invalid:"));
        assert!(read_token_file(directory.path()).is_err());
    }
}

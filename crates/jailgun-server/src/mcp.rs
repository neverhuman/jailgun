//! Standard MCP transport and protocol handling from the official Rust SDK.
mod catalog;
mod protocol;
mod tools;

use crate::AppState;
use rmcp::{
    model::*,
    service::RequestContext,
    transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    ErrorData, RoleServer, ServerHandler,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct McpService {
    state: Arc<AppState>,
}

pub(crate) fn service(
    state: Arc<AppState>,
) -> StreamableHttpService<McpService, LocalSessionManager> {
    StreamableHttpService::new(
        move || {
            Ok(McpService {
                state: state.clone(),
            })
        },
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false)
            .with_json_response(true)
            .with_sse_keep_alive(None)
            .with_max_request_body_bytes(512 * 1024),
    )
}

pub(crate) fn protocol_headers_valid(headers: &axum::http::HeaderMap) -> bool {
    let values = headers
        .get_all("mcp-protocol-version")
        .iter()
        .collect::<Vec<_>>();
    values.is_empty()
        || (values.len() == 1
            && values[0].to_str().is_ok_and(|version| {
                ProtocolVersion::known_up_to(&ProtocolVersion::V_2025_11_25)
                    .iter()
                    .any(|known| known.as_str() == version)
            }))
}

impl ServerHandler for McpService {
    fn supported_protocol_versions(&self) -> std::borrow::Cow<'static, [ProtocolVersion]> {
        std::borrow::Cow::Borrowed(ProtocolVersion::known_up_to(&ProtocolVersion::V_2025_11_25))
    }
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2025_11_25)
            .with_server_info(Implementation::new("jailgun",env!("CARGO_PKG_VERSION")))
            .with_instructions("Concept workflows never authorize deployment. Account login requires the operator dashboard. Reuse an idempotency key only for identical submissions.")
    }
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let principal = protocol::principal(&context)?;
        let tools = catalog::catalog()
            .into_iter()
            .filter(|tool| self.tool_available(&tool.name))
            .filter(|tool| !catalog::operator_only(&tool.name) || principal.is_operator())
            .collect();
        Ok(ListToolsResult {
            tools,
            ..Default::default()
        })
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        catalog::catalog()
            .into_iter()
            .find(|tool| tool.name == name && self.tool_available(&tool.name))
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let principal = protocol::principal(&context)?;
        if self.get_tool(&request.name).is_none() {
            return Err(ErrorData::invalid_params("unknown tool", None));
        }
        let result = tools::call(
            &self.state,
            &principal,
            &request.name,
            request.arguments.unwrap_or_default(),
            &context,
        )
        .await?;
        Ok(result.into())
    }
}

impl McpService {
    fn tool_available(&self, name: &str) -> bool {
        if catalog::worker_tool(name) {
            return self.state.worker.is_some();
        }
        !(self.state.worker.is_some() && self.state.workflow.is_none())
    }
}

mod protocol;
mod tools;

use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

use crate::{
    mcp::protocol::{mcp_error_response, mcp_result_response},
    mcp::tools::{mcp_call_tool, mcp_tool_list},
    state::AppState,
};

pub(crate) async fn post_mcp(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let Some(request) = body.as_object() else {
        return mcp_error_response(
            None,
            -32600,
            "invalid request",
            Some(json!({
                "reason": "request body must be a JSON object",
            })),
        );
    };
    let request_id = request.get("id").cloned();
    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return mcp_error_response(
            request_id,
            -32600,
            "invalid request",
            Some(json!({
                "reason": "request method is required",
            })),
        );
    };

    match method {
        "initialize" => mcp_result_response(
            request_id,
            json!({
                "protocolVersion": "2025-06-18",
                "serverInfo": {
                    "name": "jailgun",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "capabilities": {
                    "tools": { "listChanged": false },
                },
            }),
        ),
        "notifications/initialized" => StatusCode::NO_CONTENT.into_response(),
        "tools/list" => mcp_result_response(
            request_id,
            json!({
                "tools": mcp_tool_list(),
            }),
        ),
        "tools/call" => mcp_call_tool(state, headers, request_id, request.get("params")).await,
        other => mcp_error_response(
            request_id,
            -32601,
            "method not found",
            Some(json!({ "method": other })),
        ),
    }
}

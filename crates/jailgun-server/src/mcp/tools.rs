use std::sync::Arc;

use axum::{
    extract::{Path as AxumPath, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

use crate::{
    browser::{browser_write_unauthorized, get_browser_account, post_browser_account_auth_code},
    mcp::protocol::{mcp_error_response, mcp_tool_response},
    runs::{get_agent_summary, get_run, start_agent_run_inner, RunIngress},
    state::AppState,
};

pub(super) fn mcp_tool_list() -> Vec<Value> {
    vec![
        json!({
            "name": "jailgun.run",
            "title": "Start Jailgun run",
            "description": "Start a Jailgun agent run using the published REST contract.",
            "inputSchema": { "type": "object" },
        }),
        json!({
            "name": "jailgun.run_status",
            "title": "Get run status",
            "description": "Return the current run snapshot for a run_id.",
            "inputSchema": {
                "type": "object",
                "properties": { "run_id": { "type": "string" } },
                "required": ["run_id"],
            },
        }),
        json!({
            "name": "jailgun.run_summary",
            "title": "Get run summary",
            "description": "Return the agent summary for a run_id.",
            "inputSchema": {
                "type": "object",
                "properties": { "run_id": { "type": "string" } },
                "required": ["run_id"],
            },
        }),
        json!({
            "name": "jailgun.auth_status",
            "title": "Get browser auth status",
            "description": "Return the current browser account auth status.",
            "inputSchema": {
                "type": "object",
                "properties": { "account_id": { "type": "string" } },
                "required": ["account_id"],
            },
        }),
        json!({
            "name": "jailgun.submit_code",
            "title": "Submit auth code",
            "description": "Submit a browser auth code for the active auth session.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "account_id": { "type": "string" },
                    "code": { "type": "string" },
                },
                "required": ["account_id", "code"],
            },
        }),
    ]
}

pub(super) async fn mcp_call_tool(
    state: Arc<AppState>,
    headers: HeaderMap,
    request_id: Option<Value>,
    params: Option<&Value>,
) -> Response {
    let Some(params) = params.and_then(Value::as_object) else {
        return mcp_error_response(
            request_id,
            -32602,
            "invalid params",
            Some(json!({
                "reason": "tools/call requires params.name",
            })),
        );
    };
    let Some(tool_name) = params.get("name").and_then(Value::as_str) else {
        return mcp_error_response(
            request_id,
            -32602,
            "invalid params",
            Some(json!({
                "reason": "tools/call requires params.name",
            })),
        );
    };
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let response = match tool_name {
        "jailgun.run" => start_agent_run_inner(
            State(state.clone()),
            headers.clone(),
            arguments,
            RunIngress::Mcp,
        )
        .await
        .into_response(),
        "jailgun.run_status" => {
            match mcp_call_run_status(&state, request_id.clone(), arguments).await {
                Ok(response) => response,
                Err(response) => return response,
            }
        }
        "jailgun.run_summary" => {
            match mcp_call_run_summary(&state, request_id.clone(), arguments).await {
                Ok(response) => response,
                Err(response) => return response,
            }
        }
        "jailgun.auth_status" => {
            match mcp_call_auth_status(&state, headers.clone(), request_id.clone(), arguments).await
            {
                Ok(response) => response,
                Err(response) => return response,
            }
        }
        "jailgun.submit_code" => {
            match mcp_call_submit_code(&state, headers.clone(), request_id.clone(), arguments).await
            {
                Ok(response) => response,
                Err(response) => return response,
            }
        }
        other => {
            return mcp_error_response(
                request_id,
                -32601,
                "unknown tool",
                Some(json!({ "tool": other })),
            );
        }
    };
    mcp_tool_response(request_id, response).await
}

async fn mcp_call_run_status(
    state: &Arc<AppState>,
    request_id: Option<Value>,
    arguments: Value,
) -> Result<Response, Response> {
    let Some(run_id) = arguments.get("run_id").and_then(Value::as_str) else {
        return Err(required_param_error(request_id, "run_id"));
    };
    let response = get_run(State(state.clone()), AxumPath(run_id.to_string())).await;
    Ok(response.into_response())
}

async fn mcp_call_run_summary(
    state: &Arc<AppState>,
    request_id: Option<Value>,
    arguments: Value,
) -> Result<Response, Response> {
    let Some(run_id) = arguments.get("run_id").and_then(Value::as_str) else {
        return Err(required_param_error(request_id, "run_id"));
    };
    let response = get_agent_summary(State(state.clone()), AxumPath(run_id.to_string())).await;
    Ok(response.into_response())
}

async fn mcp_call_auth_status(
    state: &Arc<AppState>,
    headers: HeaderMap,
    request_id: Option<Value>,
    arguments: Value,
) -> Result<Response, Response> {
    let Some(account_id) = arguments.get("account_id").and_then(Value::as_str) else {
        return Err(required_param_error(request_id, "account_id"));
    };
    if let Some(response) = browser_write_unauthorized(state, &headers) {
        return Ok(response);
    }
    let response = get_browser_account(
        State(state.clone()),
        AxumPath(account_id.to_string()),
        headers,
    )
    .await;
    Ok(response.into_response())
}

async fn mcp_call_submit_code(
    state: &Arc<AppState>,
    headers: HeaderMap,
    request_id: Option<Value>,
    arguments: Value,
) -> Result<Response, Response> {
    let Some(account_id) = arguments.get("account_id").and_then(Value::as_str) else {
        return Err(required_param_error(request_id, "account_id"));
    };
    let Some(code) = arguments.get("code").and_then(Value::as_str) else {
        return Err(required_param_error(request_id, "code"));
    };
    if let Some(response) = browser_write_unauthorized(state, &headers) {
        return Ok(response);
    }
    let response = post_browser_account_auth_code(
        State(state.clone()),
        AxumPath(account_id.to_string()),
        headers,
        Json(json!({ "code": code })),
    )
    .await;
    Ok(response.into_response())
}

fn required_param_error(request_id: Option<Value>, name: &str) -> Response {
    mcp_error_response(
        request_id,
        -32602,
        "invalid params",
        Some(json!({
            "reason": format!("{name} is required"),
        })),
    )
}

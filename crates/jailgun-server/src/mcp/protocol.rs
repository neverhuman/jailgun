use axum::{
    body::to_bytes,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

pub(super) fn mcp_result_response(id: Option<Value>, result: Value) -> Response {
    Json(json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "result": result,
    }))
    .into_response()
}

pub(super) fn mcp_error_response(
    id: Option<Value>,
    code: i64,
    message: impl Into<String>,
    data: Option<Value>,
) -> Response {
    let mut error = json!({
        "code": code,
        "message": message.into(),
    });
    if let Some(data) = data {
        error["data"] = data;
    }
    Json(json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": error,
    }))
    .into_response()
}

pub(super) async fn mcp_tool_response(request_id: Option<Value>, response: Response) -> Response {
    let status = response.status();
    let body = match to_bytes(response.into_body(), usize::MAX).await {
        Ok(bytes) => bytes,
        Err(error) => {
            return mcp_error_response(
                request_id,
                -32603,
                "internal error",
                Some(json!({ "reason": error.to_string() })),
            );
        }
    };
    let text = String::from_utf8_lossy(&body).trim().to_string();
    let parsed = serde_json::from_slice::<Value>(&body).ok();
    let content_text = if text.is_empty() {
        parsed.as_ref().map(Value::to_string).unwrap_or_default()
    } else {
        text
    };
    let mut result = json!({
        "content": [{ "type": "text", "text": content_text }],
    });
    if let Some(value) = parsed {
        result["structuredContent"] = value;
    }
    if !status.is_success() {
        result["isError"] = Value::Bool(true);
    }
    mcp_result_response(request_id, result)
}

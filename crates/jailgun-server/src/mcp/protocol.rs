use crate::auth::Principal;
use rmcp::{model::CallToolResult, service::RequestContext, ErrorData, RoleServer};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{Map, Value};

pub(super) fn principal(context: &RequestContext<RoleServer>) -> Result<Principal, ErrorData> {
    context
        .extensions
        .get::<axum::http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<Principal>())
        .cloned()
        .ok_or_else(|| {
            ErrorData::internal_error("authenticated request context is unavailable", None)
        })
}
pub(super) fn input<T: DeserializeOwned>(arguments: Map<String, Value>) -> Result<T, ErrorData> {
    serde_json::from_value(Value::Object(arguments))
        .map_err(|_| ErrorData::invalid_params("arguments do not match the tool schema", None))
}
pub(super) fn output<T: Serialize>(
    result: Result<T, jailgun_workflow::Error>,
) -> Result<CallToolResult, ErrorData> {
    match result {
        Ok(value) => serde_json::to_value(value)
            .map(CallToolResult::structured)
            .map_err(|_| ErrorData::internal_error("result serialization failed", None)),
        Err(error) => Ok(CallToolResult::structured_error(
            serde_json::to_value(error.response()).expect("error response"),
        )),
    }
}
pub(super) async fn response(
    response: axum::response::Response,
) -> Result<CallToolResult, ErrorData> {
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .map_err(|_| ErrorData::internal_error("result exceeds the response limit", None))?;
    let value = serde_json::from_slice(&bytes)
        .map_err(|_| ErrorData::internal_error("result is not JSON", None))?;
    Ok(if status.is_success() {
        CallToolResult::structured(value)
    } else {
        CallToolResult::structured_error(value)
    })
}

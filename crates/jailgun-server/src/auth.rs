//! One authentication boundary for REST, MCP, and dashboard WebSockets.
mod access;
pub(crate) use access::Principal;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::AppState;

const COOKIE: &str = "jailgun_session";
const SESSION_SECONDS: u64 = 8 * 60 * 60;

#[derive(Default)]
pub struct DashboardSessions {
    codes: Mutex<HashMap<String, Instant>>,
    sessions: Mutex<HashMap<String, Instant>>,
}

impl DashboardSessions {
    pub fn issue_pairing_code(&self) -> String {
        let code = uuid::Uuid::new_v4().to_string();
        let mut codes = self.codes.lock().expect("pairing lock");
        // Only the latest local pairing request is valid.
        codes.clear();
        codes.insert(hash(&code), Instant::now() + Duration::from_secs(300));
        code
    }

    fn exchange(&self, code: &str) -> Option<String> {
        let expires = self.codes.lock().ok()?.remove(&hash(code))?;
        if Instant::now() >= expires {
            return None;
        }
        let session = uuid::Uuid::new_v4().to_string();
        let mut sessions = self.sessions.lock().ok()?;
        sessions.retain(|_, expires| *expires > Instant::now());
        if sessions.len() >= 64 {
            return None;
        }
        sessions.insert(
            hash(&session),
            Instant::now() + Duration::from_secs(SESSION_SECONDS),
        );
        Some(session)
    }

    pub(crate) fn accepts(&self, headers: &HeaderMap) -> bool {
        let Some(cookie) = headers.get("cookie").and_then(|v| v.to_str().ok()) else {
            return false;
        };
        let mut values = cookie
            .split(';')
            .filter_map(|part| part.trim().split_once('='))
            .filter(|(key, _)| *key == COOKIE)
            .map(|(_, value)| value);
        let Some(value) = values.next() else {
            return false;
        };
        if values.next().is_some() {
            return false;
        }
        self.sessions
            .lock()
            .ok()
            .and_then(|sessions| sessions.get(&hash(value)).copied())
            .is_some_and(|expiry| expiry > Instant::now())
    }
}

fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

pub(crate) fn unauthorized(state: &AppState, headers: &HeaderMap) -> Option<Response> {
    let Some(expected) = state.ingest_token.as_deref().filter(|v| !v.is_empty()) else {
        return Some(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "authentication-unconfigured",
            "Start the service with an operator credential.",
        ));
    };
    let compatibility_token = headers.get("x-jailgun-token").and_then(|v| v.to_str().ok());
    let authorization = headers.get("authorization").and_then(|v| v.to_str().ok());
    let bearer = authorization
        .and_then(|v| v.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, token)| token);
    if headers.contains_key("x-jailgun-token") || headers.contains_key("authorization") {
        let valid = compatibility_token
            .or(bearer)
            .is_some_and(|provided| hash(provided) == hash(expected))
            && compatibility_token.zip(bearer).is_none_or(|(a, b)| a == b)
            && (authorization.is_none() || bearer.is_some());
        if valid {
            return None;
        }
    } else if state.dashboard_sessions.accepts(headers) {
        return None;
    }
    Some(error(StatusCode::UNAUTHORIZED, "unauthorized", "Pair this browser using the code printed by the local service, or supply a valid bearer token."))
}

/// Restrict browser access to the loopback origin used by local/SSH-forwarded UI.
pub(crate) fn origin_allowed(headers: &HeaderMap) -> bool {
    let Some(host) = headers.get("host").and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let Ok(authority) = host.parse::<axum::http::uri::Authority>() else {
        return false;
    };
    if !matches!(
        authority.host(),
        "localhost" | "127.0.0.1" | "[::1]" | "::1"
    ) {
        return false;
    }
    if headers
        .get("sec-fetch-site")
        .is_some_and(|v| v == "cross-site")
    {
        return false;
    }
    let Some(origin) = headers.get("origin") else {
        return true;
    };
    let Some(origin) = origin
        .to_str()
        .ok()
        .and_then(|v| v.parse::<axum::http::Uri>().ok())
    else {
        return false;
    };
    origin.scheme_str() == Some("http")
        && origin.authority().is_some_and(|a| a.as_str() == host)
        && origin.path() == "/"
        && origin.query().is_none()
}

pub(crate) async fn protect(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Response {
    if !origin_allowed(request.headers()) {
        return error(
            StatusCode::FORBIDDEN,
            "origin-rejected",
            "Use the loopback dashboard address or an SSH local forward.",
        );
    }
    let principal = match access::authenticate(&state, request.headers()).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    if let Err(response) =
        access::authorize(&state, &principal, request.method(), request.uri()).await
    {
        return response;
    }
    // Cookie mutations and WebSockets must carry a browser Origin, protecting
    // against same-site cross-port requests as well as cross-site requests.
    let is_cookie = matches!(
        &principal,
        Principal::Operator {
            browser_session: true
        }
    );
    if is_cookie
        && (request.method() != axum::http::Method::GET || request.uri().path().starts_with("/ws/"))
        && !request.headers().contains_key("origin")
    {
        return error(
            StatusCode::FORBIDDEN,
            "origin-required",
            "Open the dashboard from its loopback address.",
        );
    }
    if request.uri().path().trim_end_matches('/') == "/mcp"
        && !crate::mcp::protocol_headers_valid(request.headers())
    {
        return error(
            StatusCode::BAD_REQUEST,
            "unsupported-protocol-version",
            "Use one supported MCP-Protocol-Version header (2025-11-25 recommended).",
        );
    }
    request.extensions_mut().insert(principal);
    if state
        .daemon_control
        .as_ref()
        .is_some_and(|control| control.is_stopping())
        && request.uri().path() != "/api/service/stop"
        && (request.method() != axum::http::Method::GET || request.uri().path() == "/mcp")
    {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "service-stopping",
            "Wait for shutdown to finish before restarting the daemon.",
        );
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response
}

pub(crate) async fn pairing_code(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({"code":state.dashboard_sessions.issue_pairing_code(),"expires_in_seconds":300}))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairRequest {
    code: String,
}

pub(crate) async fn pair(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<PairRequest>,
) -> Response {
    if !origin_allowed(&headers) || !headers.contains_key("origin") {
        return error(
            StatusCode::FORBIDDEN,
            "origin-rejected",
            "Pair from the loopback dashboard.",
        );
    }
    if state.ingest_token.as_deref().is_none_or(str::is_empty) {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "authentication-unconfigured",
            "Start the service with an operator credential.",
        );
    }
    let Some(session) = state.dashboard_sessions.exchange(&body.code) else {
        return error(StatusCode::UNAUTHORIZED, "pairing-code-invalid", "Request a new code from the local service; codes expire after five minutes and work once.");
    };
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        "set-cookie",
        HeaderValue::from_str(&format!(
            "{COOKIE}={session}; HttpOnly; SameSite=Strict; Path=/; Max-Age={SESSION_SECONDS}"
        ))
        .expect("UUID cookie"),
    );
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response
}

pub(crate) fn error(status: StatusCode, code: &str, next_action: &str) -> Response {
    (
        status,
        Json(json!({ "error": code, "code": code, "next_action": next_action })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_codes_and_sessions_fail_closed() {
        let sessions = DashboardSessions::default();
        let code = sessions.issue_pairing_code();
        sessions
            .codes
            .lock()
            .unwrap()
            .insert(hash(&code), Instant::now() - Duration::from_secs(1));
        assert!(sessions.exchange(&code).is_none());
        let code = sessions.issue_pairing_code();
        let session = sessions.exchange(&code).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("cookie", format!("{COOKIE}={session}").parse().unwrap());
        assert!(sessions.accepts(&headers));
        sessions
            .sessions
            .lock()
            .unwrap()
            .insert(hash(&session), Instant::now() - Duration::from_secs(1));
        assert!(!sessions.accepts(&headers));
    }
}

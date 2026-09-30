use super::{error, unauthorized};
use crate::AppState;
use axum::{
    http::{HeaderMap, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use jailgun_workflow::model::TokenMetadata;

#[derive(Clone)]
pub(crate) enum Principal {
    Operator { browser_session: bool },
    Automation(TokenMetadata),
}

impl Principal {
    pub fn permits_account(&self, id: &str) -> bool {
        match self {
            Self::Operator { .. } => true,
            Self::Automation(token) => token.account_ids.iter().any(|account| account == id),
        }
    }
    pub fn is_operator(&self) -> bool {
        matches!(self, Self::Operator { .. })
    }
    pub fn require_account(&self, id: &str) -> Result<(), jailgun_workflow::Error> {
        if self.permits_account(id) {
            Ok(())
        } else {
            Err(jailgun_workflow::Error::action(
                "account-forbidden",
                "This automation token does not permit access to that account.",
                "Use a token authorized for this account.",
            ))
        }
    }
}

pub(crate) async fn authenticate(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Principal, Response> {
    let supplied = token(headers).map_err(|()| {
        error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "Supply one unambiguous bearer or x-jailgun-token credential.",
        )
    })?;
    if unauthorized(state, headers).is_none() {
        return Ok(Principal::Operator {
            browser_session: supplied.is_none(),
        });
    }
    if state.ingest_token.as_deref().is_none_or(str::is_empty) {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "authentication-unconfigured",
            "Start the service with an operator credential.",
        ));
    }
    if let (Some(secret), Some(store)) = (supplied, state.workflow.as_ref()) {
        match store.authenticate_token(secret.to_string()).await {
            Ok(Some(token)) => return Ok(Principal::Automation(token)),
            Err(_) => {
                return Err(error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "authorization-unavailable",
                    "Restart the daemon and inspect private storage diagnostics.",
                ))
            }
            Ok(None) => {}
        }
    }
    Err(error(
        StatusCode::UNAUTHORIZED,
        "unauthorized",
        "Pair the dashboard or supply a valid, unrevoked credential.",
    ))
}

fn token(headers: &HeaderMap) -> Result<Option<&str>, ()> {
    for name in ["authorization", "x-jailgun-token"] {
        if headers.get_all(name).iter().count() > 1 {
            return Err(());
        }
    }
    let compatibility = headers
        .get("x-jailgun-token")
        .map(|v| v.to_str().map_err(|_| ()))
        .transpose()?;
    let bearer = headers
        .get("authorization")
        .map(|v| {
            let (scheme, value) = v.to_str().map_err(|_| ())?.split_once(' ').ok_or(())?;
            if !scheme.eq_ignore_ascii_case("bearer")
                || value.is_empty()
                || value.chars().any(char::is_whitespace)
            {
                return Err(());
            }
            Ok(value)
        })
        .transpose()?;
    if compatibility.zip(bearer).is_some_and(|(a, b)| a != b) {
        return Err(());
    }
    Ok(bearer.or(compatibility))
}

/// Automation permissions default to denial. Account mutation, configuration,
/// archive execution, operator WebSockets and pairing never enter this allowlist.
pub(crate) async fn authorize(
    state: &AppState,
    principal: &Principal,
    method: &Method,
    uri: &Uri,
) -> Result<(), Response> {
    if principal.is_operator() {
        return Ok(());
    }
    match (method, uri.path()) {
        (&Method::POST, "/mcp")
        | (&Method::GET, "/mcp")
        | (&Method::DELETE, "/mcp")
        | (&Method::POST, "/api/concept-runs")
        | (&Method::GET, "/api/accounts")
        | (&Method::GET, "/api/runs") => return Ok(()),
        _ => {}
    }
    let parts = uri
        .path()
        .trim_start_matches('/')
        .split('/')
        .collect::<Vec<_>>();
    let run_route =
        parts.first() == Some(&"api") && parts.get(1) == Some(&"runs") && parts.len() >= 3;
    if run_route {
        let allowed = matches!(
            (method, parts.get(3).copied(), parts.len()),
            (&Method::GET, None, 3)
                | (&Method::GET, Some("result" | "attempts" | "events"), 4)
                | (&Method::GET, Some("artifacts"), 5)
                | (&Method::POST, Some("pause" | "resume" | "cancel"), 4)
        );
        if allowed {
            if let Some(store) = &state.workflow {
                if let Ok(run) = store.run(parts[2].into()).await {
                    return principal
                        .require_account(&run.request.account_id)
                        .map_err(|error| crate::concepts::ApiError(error).into_response());
                }
            }
            return Err(error(
                StatusCode::NOT_FOUND,
                "not-found",
                "List the concept runs available to this token.",
            ));
        }
    }
    Err(error(
        StatusCode::FORBIDDEN,
        "operator-required",
        "Use the operator dashboard or operator credential for this action.",
    ))
}

use super::{
    catalog::*,
    protocol::{input, output, response},
};
use crate::{
    auth::Principal,
    concepts,
    runs::{get_agent_summary, get_run, start_agent_run_inner, RunIngress},
    AppState,
};
use axum::{
    extract::{Path, State},
    response::IntoResponse,
};
use jailgun_workflow::{
    model::{ArtifactRead, ConceptRequest},
    Error,
};
use rmcp::{model::CallToolResult, service::RequestContext, ErrorData, RoleServer};
use serde_json::{Map, Value};
use std::sync::Arc;

pub(super) async fn call(
    state: &Arc<AppState>,
    principal: &Principal,
    name: &str,
    args: Map<String, Value>,
    context: &RequestContext<RoleServer>,
) -> Result<CallToolResult, ErrorData> {
    if operator_only(name) && !principal.is_operator() {
        return output::<Value>(Err(Error::action(
            "operator-required",
            "Archive execution requires operator authorization.",
            "Use concept operations with an automation credential.",
        )));
    }
    if worker_tool(name) {
        let Some(worker) = state.worker.as_ref() else {
            return output::<Value>(Err(Error::action(
                "worker-unavailable",
                "The lightweight worker runtime is not active.",
                "Start `jailgun worker` and reconnect the MCP client.",
            )));
        };
        return match name {
            "jailgun.worker.info" => {
                let _: Empty = input(args)?;
                output(Ok(worker.info().await))
            }
            "jailgun.worker.account_register" => {
                output(worker.register_account(input(args)?).await)
            }
            "jailgun.worker.account_list" => {
                let request: crate::worker::AccountList = input(args)?;
                output(worker.accounts(request.refresh).await)
            }
            "jailgun.worker.account_refresh" => output(worker.refresh_account(input(args)?).await),
            "jailgun.worker.tab_open" => output(worker.open_tab(input(args)?).await),
            "jailgun.worker.tab_list" => {
                let _: Empty = input(args)?;
                output(Ok(worker.tabs().await))
            }
            "jailgun.worker.tab_close" => output(worker.close_tab(input(args)?).await),
            "jailgun.worker.tab_set_model" => output(worker.set_model(input(args)?).await),
            "jailgun.worker.job_submit" => output(worker.submit_job(input(args)?).await),
            "jailgun.worker.job_status" => {
                let request: crate::worker::JobRef = input(args)?;
                output(worker.job(&request.job_id).await)
            }
            "jailgun.worker.job_cancel" => output(worker.cancel_job(input(args)?).await),
            "jailgun.worker.object_put" => output(worker.put_object(input(args)?).await),
            "jailgun.worker.object_get" => output(worker.get_object(input(args)?).await),
            "jailgun.worker.object_list" => {
                let _: Empty = input(args)?;
                output(Ok(worker.objects().await))
            }
            _ => Err(ErrorData::invalid_params("unknown worker tool", None)),
        };
    }
    if name == "jailgun.run_summary" {
        let request: RunId = input(args)?;
        return response(get_agent_summary(State(state.clone()), Path(request.run_id)).await).await;
    }
    if name == "jailgun.run_status" && principal.is_operator() {
        let request: RunId = input(args)?;
        return response(get_run(State(state.clone()), Path(request.run_id)).await).await;
    }
    if name == "jailgun.run" {
        let headers = context
            .extensions
            .get::<axum::http::request::Parts>()
            .ok_or_else(|| ErrorData::internal_error("request headers unavailable", None))?
            .headers
            .clone();
        return response(
            start_agent_run_inner(
                State(state.clone()),
                headers,
                Value::Object(args),
                RunIngress::Mcp,
            )
            .await
            .into_response(),
        )
        .await;
    }
    let store = match state.workflow.as_ref() {
        Some(store) => store,
        None => {
            return output::<Value>(Err(Error::action(
                "workflow-unavailable",
                "The durable concept runtime is not active.",
                "Start jailgun serve --concepts and connect an account.",
            )))
        }
    };
    match name {
        "jailgun.brainstorm" => {
            let request: ConceptRequest = input(args)?;
            output(concepts::submit_request(state, principal, request).await)
        }
        "jailgun.run_artifact" => {
            let request: ArtifactRead = input(args)?;
            let run = match store.run(request.run_id.clone()).await {
                Ok(run) => run,
                Err(error) => return output::<Value>(Err(error)),
            };
            if let Err(error) = principal.require_account(&run.request.account_id) {
                return output::<Value>(Err(error));
            }
            output(store.artifact_chunk(request).await)
        }
        "jailgun.accounts" => {
            let _: Empty = input(args)?;
            output(store.accounts().await.map(|accounts| {
                Accounts {
                    accounts: accounts
                        .into_iter()
                        .filter(|a| principal.permits_account(&a.id))
                        .collect(),
                }
            }))
        }
        "jailgun.auth_status" => {
            let query: AuthQuery = input(args)?;
            if let Some(id) = &query.account_id {
                if let Err(error) = principal.require_account(id) {
                    return output::<Value>(Err(error));
                }
            }
            let host = context
                .extensions
                .get::<axum::http::request::Parts>()
                .and_then(|p| p.headers.get("host"))
                .and_then(|h| h.to_str().ok())
                .unwrap_or("127.0.0.1:8787");
            output(store.accounts().await.map(|accounts| {
                let accounts = accounts
                    .into_iter()
                    .filter(|a| {
                        principal.permits_account(&a.id)
                            && query.account_id.as_ref().is_none_or(|id| id == &a.id)
                    })
                    .collect::<Vec<_>>();
                let action = (!accounts.iter().any(|a| a.readiness == "ready")).then(|| {
                    "Ask the operator to connect or reconnect an account in the dashboard."
                        .to_string()
                });
                AuthStatus {
                    accounts,
                    dashboard_url: format!("http://{host}/#accounts"),
                    action,
                }
            }))
        }
        _ => {
            let resume = name == "jailgun.run_resume";
            let (id, allow_incomplete) = if resume {
                let request: Resume = input(args)?;
                (request.run_id, request.allow_incomplete)
            } else {
                let request: RunId = input(args)?;
                (request.run_id, false)
            };
            let run = match store.run(id.clone()).await {
                Ok(run) => run,
                Err(error) => return output::<Value>(Err(error)),
            };
            if let Err(error) = principal.require_account(&run.request.account_id) {
                return output::<Value>(Err(error));
            }
            match name {
                "jailgun.run_status" => output(Ok(run)),
                "jailgun.run_result" => output(store.result(id).await),
                "jailgun.run_pause" => output(store.pause(id, concepts::now()).await),
                "jailgun.run_resume" => {
                    output(store.resume(id, allow_incomplete, concepts::now()).await)
                }
                "jailgun.run_cancel" => output(store.cancel(id, concepts::now()).await),
                _ => Err(ErrorData::invalid_params("unknown tool", None)),
            }
        }
    }
}

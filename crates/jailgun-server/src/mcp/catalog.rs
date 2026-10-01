use crate::worker::*;
use jailgun_workflow::model::*;
use rmcp::model::{JsonObject, Tool};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunId {
    pub run_id: String,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Resume {
    pub run_id: String,
    #[serde(default)]
    pub allow_incomplete: bool,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Empty {}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuthQuery {
    pub account_id: Option<String>,
}
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct Accounts {
    pub accounts: Vec<AccountReadiness>,
}
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct AuthStatus {
    pub accounts: Vec<AccountReadiness>,
    pub dashboard_url: String,
    pub action: Option<String>,
}
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct ArchiveRequest {
    #[serde(flatten)]
    pub request: jailgun_core::JailgunAgentRunRequest,
    pub account: Option<String>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum RunStatus {
    Concept(Box<Run>),
    Archive(jailgun_core::RunSnapshot),
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum ArchiveSummary {
    Complete(Box<jailgun_core::JailgunAgentRunSummary>),
    Running { run_id: String, status: String },
}
pub(super) fn operator_only(name: &str) -> bool {
    matches!(name, "jailgun.run" | "jailgun.run_summary") || worker_tool(name)
}
pub(super) fn worker_tool(name: &str) -> bool {
    name.starts_with("jailgun.worker.")
}
fn schema<T: JsonSchema>() -> Arc<JsonObject> {
    let mut schema = serde_json::to_value(schema_for!(T))
        .expect("schema")
        .as_object()
        .expect("object schema")
        .clone();
    // All tool inputs/outputs are objects, including the untagged archive unions.
    schema.insert("type".into(), serde_json::json!("object"));
    Arc::new(schema)
}
fn tool<I: JsonSchema, O: JsonSchema>(name: &'static str, description: &'static str) -> Tool {
    Tool::new(name, description, schema::<I>()).with_raw_output_schema(schema::<O>())
}
pub(super) fn catalog() -> Vec<Tool> {
    vec![
        tool::<ConceptRequest,RunAccepted>("jailgun.brainstorm","Start a concept workflow with 5–10 distinct perspectives, comparison, synthesis, critique and revision. Provide a unique idempotency_key; repeating it with the same content returns the same run. No deployment is enabled."),
        tool::<RunId,RunStatus>("jailgun.run_status","Read durable stage, candidate, attempt budget and artifact status. Completed candidates and partial responses are distinct."),
        tool::<ArtifactRead,ArtifactChunk>("jailgun.run_artifact","Read registered text artifacts in bounded UTF-8 chunks. Use next_offset until eof; verify combined bytes against artifact.sha256 and byte_length. Each request rechecks authorization. No filesystem paths are accepted."),
        tool::<RunId,FinalResult>("jailgun.run_result","Read the final manifest for a completed workflow. Unfinished runs return an actionable tool error."),
        tool::<RunId,Run>("jailgun.run_pause","Pause future submissions while retaining completed results and safely capturing active conversations."),
        tool::<Resume,Run>("jailgun.run_resume","Resume eligible work without resetting budgets. Set allow_incomplete only with explicit user authorization to exclude failed candidates; at least three must be complete."),
        tool::<RunId,Run>("jailgun.run_cancel","Persistently cancel the run and stop only its owned conversations. Retain captured results; repeated cancellation is safe."),
        tool::<Empty,Accounts>("jailgun.accounts","List permitted accounts with readiness, active conversation capacity, pacing and cooldown status."),
        tool::<AuthQuery,AuthStatus>("jailgun.auth_status","Check permitted account readiness and obtain the operator dashboard action when login is needed. Account login remains an operator dashboard operation."),
        tool::<RunId,ArchiveSummary>("jailgun.run_summary","Read an advanced archive run summary. Requires operator credentials; use run_result for concept workflows."),
        tool::<ArchiveRequest,crate::JailgunAgentRunAcceptedResponse>("jailgun.run","Advanced compatibility operation for archive capture and guarded deployment. Requires operator credentials, explicit execution settings, and an archive runtime separate from managed concept accounts."),
        tool::<Empty,WorkerInfo>("jailgun.worker.info","Read worker limits, executor identity, model aliases, reasoning efforts, and cached account-readiness counts."),
        tool::<AccountRegister,WorkerAccount>("jailgun.worker.account_register","Browser accounts are created only in the operator dashboard. This compatibility tool returns an actionable dashboard-account-required error in browser worker mode."),
        tool::<AccountList,WorkerAccounts>("jailgun.worker.account_list","List dashboard-managed ChatGPT accounts and total, ready, login-required, and unavailable counts. Refreshes supervised browser readiness by default."),
        tool::<AccountRef,WorkerAccount>("jailgun.worker.account_refresh","Recheck one supervised ChatGPT browser account without exposing cookies or credentials."),
        tool::<TabOpen,WorkerTab>("jailgun.worker.tab_open","Open one account-bound logical browser tab. Optionally attach a verified tar.gz object to the ChatGPT prompt."),
        tool::<Empty,WorkerTabs>("jailgun.worker.tab_list","List logical tabs, defaults, state, and active job ownership."),
        tool::<TabRef,WorkerTab>("jailgun.worker.tab_close","Close a logical tab. Busy tabs fail unless force=true, which requests cancellation first."),
        tool::<TabModel,WorkerTab>("jailgun.worker.tab_set_model","Set the model and reasoning effort for an idle open tab. Aliases include astra and sol."),
        tool::<JobSubmit,WorkerJob>("jailgun.worker.job_submit","Submit one idempotent ChatGPT browser job to an isolated logical tab with model, reasoning effort, timeout, and error-reporting controls."),
        tool::<JobRef,WorkerJob>("jailgun.worker.job_status","Read queued, running, cancelling, completed, failed, timed-out, or cancelled job status and its output object ID."),
        tool::<JobRef,WorkerJob>("jailgun.worker.job_cancel","Request idempotent cancellation. Completed jobs and other terminal states are returned unchanged."),
        tool::<ObjectPut,ObjectPutResult>("jailgun.worker.object_put","Upload a tar.gz in ordered base64 chunks. The final chunk verifies total size, SHA-256, gzip/tar structure, and safe paths."),
        tool::<ObjectGet,ObjectChunk>("jailgun.worker.object_get","Download a verified input or output tar.gz in bounded base64 chunks. Follow next_offset and verify full SHA-256."),
        tool::<Empty,WorkerObjects>("jailgun.worker.object_list","List verified input and output objects with names, sizes, media types, and SHA-256 digests."),
    ]
}

use jailgun_workflow::{model::*, ErrorResponse};
use schemars::{schema_for, JsonSchema};

#[allow(dead_code)]
#[derive(JsonSchema)]
struct WorkflowContracts {
    service_status: jailgun_core::installation::ServiceStatus,
    service_stop_request: jailgun_core::installation::ServiceStopRequest,
    concept_request: ConceptRequest,
    run_accepted: RunAccepted,
    resume_request: ResumeRequest,
    run: Run,
    attempt: Attempt,
    comparison: Comparison,
    final_result: FinalResult,
    artifact_read: ArtifactRead,
    artifact_chunk: ArtifactChunk,
    account_readiness: AccountReadiness,
    account_session: AccountSession,
    connect_account: ConnectAccount,
    confirm_account: ConfirmAccount,
    issue_token: IssueToken,
    created_token: CreatedToken,
    token_metadata: TokenMetadata,
    event: Event,
    error: ErrorResponse,
}

fn main() {
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "event":schema_for!(jailgun_core::JailgunEvent),
            "workflow":schema_for!(WorkflowContracts),
        }))
        .expect("schema serialization")
    );
}

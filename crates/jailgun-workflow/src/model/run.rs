use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CriterionScore {
    pub criterion: String,
    pub score: u16,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CandidateEvaluation {
    pub candidate: u16,
    pub scores: Vec<CriterionScore>,
    pub disagreements: Vec<String>,
    pub uncertainty: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Run {
    pub id: String,
    pub request: ConceptRequest,
    pub configuration: ConfigurationSnapshot,
    pub status: String,
    pub pause_reason: Option<String>,
    pub created_ms: i64,
    pub deadline_ms: i64,
    pub submissions: u16,
    pub submission_limit: u16,
    pub allow_incomplete: bool,
    pub tasks: Vec<Task>,
    pub artifacts: Vec<Artifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Task {
    pub id: String,
    pub stage: String,
    pub position: u16,
    pub status: String,
    pub attempts: u16,
    pub summary: Option<CandidateSummary>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Attempt {
    pub id: String,
    pub run_id: String,
    pub task_id: String,
    pub number: u16,
    pub state: String,
    pub conversation_id: Option<String>,
    pub conversation_url: Option<String>,
    pub user_turn_id: Option<String>,
    pub observed_model: Option<String>,
    pub created_ms: i64,
    pub accepted_ms: Option<i64>,
    pub completed_ms: Option<i64>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AcceptedTurn {
    pub conversation_id: String,
    pub conversation_url: String,
    pub user_turn_id: String,
    pub observed_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Lease {
    pub reservation_id: String,
    pub account_id: String,
    pub run_id: String,
    pub task_id: String,
    pub attempt_id: String,
    pub stage: String,
    pub position: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Artifact {
    pub id: String,
    pub run_id: String,
    pub attempt_id: Option<String>,
    pub name: String,
    pub media_type: String,
    pub sha256: String,
    pub byte_length: u64,
    pub completion: String,
    pub created_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Event {
    pub sequence: i64,
    pub run_id: String,
    pub kind: String,
    pub data: serde_json::Value,
    pub created_ms: i64,
}

#[derive(Debug, Clone)]
pub struct Capture {
    pub markdown: String,
    pub summary: Option<CandidateSummary>,
    pub evaluations: Option<Vec<CandidateEvaluation>>,
    pub conversation_id: String,
    pub conversation_url: String,
    pub observed_model: String,
    pub complete: bool,
    /// Preserve the original adapter error even when partial text is available.
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AccountReadiness {
    pub id: String,
    pub readiness: String,
    pub capacity: u16,
    pub active: u16,
    pub next_submit_ms: i64,
    pub cooldown_until_ms: i64,
    pub rate_limit_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRead {
    pub run_id: String,
    pub artifact_id: String,
    /// UTF-8 byte offset, always use next_offset from the preceding response.
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "artifact_chunk_limit")]
    #[schemars(range(min = 4, max = 16384))]
    pub limit: u32,
}
fn artifact_chunk_limit() -> u32 {
    16384
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArtifactChunk {
    pub artifact: Artifact,
    pub offset: u64,
    pub next_offset: Option<u64>,
    pub eof: bool,
    pub text: String,
}

use super::*;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Low,
    #[default]
    Medium,
    High,
    Xhigh,
    Max,
    Ultra,
}

impl ReasoningEffort {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
            Self::Max => "max",
            Self::Ultra => "ultra",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ErrorReporting {
    Off,
    #[default]
    Summary,
    Detailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkerInfo {
    pub interface_version: u16,
    pub executor: String,
    pub model_aliases: BTreeMap<String, String>,
    pub reasoning_efforts: Vec<String>,
    pub max_object_bytes: u64,
    pub max_chunk_bytes: usize,
    pub max_timeout_seconds: u64,
    pub accounts: WorkerAccountCounts,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum WorkerAccountStatus {
    Ready,
    LoginRequired,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkerAccount {
    pub account_id: String,
    pub label: String,
    pub status: WorkerAccountStatus,
    pub last_checked_ms: Option<i64>,
    pub active_tabs: usize,
    pub selected_model: Option<String>,
    pub available_models: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct WorkerAccountCounts {
    pub total: usize,
    pub ready: usize,
    pub login_required: usize,
    pub unavailable: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkerAccounts {
    pub counts: WorkerAccountCounts,
    pub accounts: Vec<WorkerAccount>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AccountRegister {
    pub account_id: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AccountList {
    #[serde(default = "default_true")]
    pub refresh: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AccountRef {
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkerTab {
    pub tab_id: String,
    pub account_id: String,
    pub status: String,
    pub model: String,
    pub reasoning_effort: ReasoningEffort,
    pub active_job_id: Option<String>,
    pub input_object_id: Option<String>,
    pub created_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkerTabs {
    pub tabs: Vec<WorkerTab>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TabOpen {
    #[serde(default)]
    pub tab_id: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub reasoning_effort: ReasoningEffort,
    #[serde(default)]
    pub input_object_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TabRef {
    pub tab_id: String,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TabModel {
    pub tab_id: String,
    pub model: String,
    pub reasoning_effort: ReasoningEffort,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JobSubmit {
    pub prompt: String,
    #[serde(default)]
    pub tab_id: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<ReasoningEffort>,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub error_reporting: ErrorReporting,
    #[serde(default)]
    pub input_object_id: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JobRef {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkerJob {
    pub job_id: String,
    pub tab_id: String,
    pub account_id: String,
    pub status: String,
    pub model: String,
    pub reasoning_effort: ReasoningEffort,
    pub timeout_seconds: u64,
    pub error_reporting: ErrorReporting,
    pub output_object_id: Option<String>,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
    pub created_ms: i64,
    pub started_ms: Option<i64>,
    pub finished_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkerObject {
    pub object_id: String,
    pub name: String,
    pub kind: String,
    pub media_type: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub created_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkerObjects {
    pub objects: Vec<WorkerObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectPut {
    #[serde(default)]
    pub upload_id: Option<String>,
    pub name: String,
    pub offset: u64,
    pub data_base64: String,
    pub final_chunk: bool,
    pub total_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ObjectPutResult {
    pub upload_id: String,
    pub next_offset: u64,
    pub committed: bool,
    pub object: Option<WorkerObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectGet {
    pub object_id: String,
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_chunk_limit")]
    #[schemars(range(min = 1, max = 262144))]
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ObjectChunk {
    pub object: WorkerObject,
    pub offset: u64,
    pub next_offset: Option<u64>,
    pub eof: bool,
    pub data_base64: String,
}

#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    pub job_id: String,
    pub tab_id: String,
    pub account_id: String,
    pub workspace: PathBuf,
    pub log_dir: PathBuf,
    pub input_archive_path: Option<PathBuf>,
    pub prompt: String,
    pub model: String,
    pub reasoning_effort: ReasoningEffort,
    pub timeout_seconds: u64,
    pub error_reporting: ErrorReporting,
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub exit_code: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionFailureKind {
    Cancelled,
    TimedOut,
    Authentication,
    Failed,
}

#[derive(Debug, Clone)]
pub struct ExecutionFailure {
    pub kind: ExecutionFailureKind,
    pub message: String,
}

#[async_trait]
pub trait WorkerExecutor: Send + Sync {
    fn name(&self) -> &'static str;
    async fn auth_status(&self, _account_id: &str) -> WorkerAccountStatus {
        WorkerAccountStatus::Ready
    }
    async fn execute(
        &self,
        request: ExecutionRequest,
        cancel: watch::Receiver<bool>,
    ) -> std::result::Result<ExecutionResult, ExecutionFailure>;
}

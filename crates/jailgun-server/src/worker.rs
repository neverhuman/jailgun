//! Lightweight, operator-only agent worker used by the MCP control plane.
//!
//! Each logical tab owns one private workspace and at most one active job. The
//! production executor invokes Codex without a shell; tests inject a fake
//! executor and never require credentials, a browser, or a live model.

use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use jailgun_workflow::{Error, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tar::{Archive, Builder};
use tokio::{
    io::AsyncWriteExt,
    process::Command,
    sync::{watch, Mutex, RwLock},
};

const MAX_OBJECT_BYTES: u64 = 256 * 1024 * 1024;
const MAX_CHUNK_BYTES: usize = 256 * 1024;
const MAX_PROMPT_CHARS: usize = 128_000;
const DEFAULT_TIMEOUT_SECONDS: u64 = 30 * 60;
const MAX_TIMEOUT_SECONDS: u64 = 24 * 60 * 60;
const AUTH_CHECK_TIMEOUT_SECONDS: u64 = 10;

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
    fn as_str(self) -> &'static str {
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
    pub workspace: PathBuf,
    pub codex_home: PathBuf,
    pub log_dir: PathBuf,
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
    async fn auth_status(&self, _codex_home: PathBuf) -> WorkerAccountStatus {
        WorkerAccountStatus::Ready
    }
    async fn execute(
        &self,
        request: ExecutionRequest,
        cancel: watch::Receiver<bool>,
    ) -> std::result::Result<ExecutionResult, ExecutionFailure>;
}

#[derive(Debug, Clone)]
pub struct ProcessExecutor {
    program: PathBuf,
}

impl ProcessExecutor {
    pub fn new(program: PathBuf) -> Self {
        Self { program }
    }
}

#[async_trait]
impl WorkerExecutor for ProcessExecutor {
    fn name(&self) -> &'static str {
        "codex-process"
    }

    async fn auth_status(&self, codex_home: PathBuf) -> WorkerAccountStatus {
        let status = tokio::time::timeout(
            Duration::from_secs(AUTH_CHECK_TIMEOUT_SECONDS),
            Command::new(&self.program)
                .arg("login")
                .arg("status")
                .env("CODEX_HOME", codex_home)
                .env_remove("OPENAI_API_KEY")
                .env_remove("CODEX_ACCESS_TOKEN")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status(),
        )
        .await;
        match status {
            Ok(Ok(status)) if status.success() => WorkerAccountStatus::Ready,
            Ok(Ok(_)) => WorkerAccountStatus::LoginRequired,
            _ => WorkerAccountStatus::Unavailable,
        }
    }

    async fn execute(
        &self,
        request: ExecutionRequest,
        mut cancel: watch::Receiver<bool>,
    ) -> std::result::Result<ExecutionResult, ExecutionFailure> {
        tokio::fs::create_dir_all(&request.log_dir)
            .await
            .map_err(io_failure)?;
        let stdout_path = request.log_dir.join("stdout.jsonl");
        let stderr_path = request.log_dir.join("stderr.log");
        let stdout = File::create(&stdout_path).map_err(io_failure)?;
        let stderr = File::create(&stderr_path).map_err(io_failure)?;
        let result_path = request.workspace.join("jailgun-result.md");
        let effort = format!(
            "model_reasoning_effort=\"{}\"",
            request.reasoning_effort.as_str()
        );
        let mut child = Command::new(&self.program)
            .arg("exec")
            .arg("--json")
            .arg("--ephemeral")
            .arg("--skip-git-repo-check")
            .arg("--sandbox")
            .arg("workspace-write")
            .arg("--model")
            .arg(&request.model)
            .arg("--config")
            .arg(effort)
            .arg("--cd")
            .arg(&request.workspace)
            .arg("--output-last-message")
            .arg(&result_path)
            .arg("-")
            .env("CODEX_HOME", &request.codex_home)
            .env_remove("OPENAI_API_KEY")
            .env_remove("CODEX_ACCESS_TOKEN")
            .stdin(Stdio::piped())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| ExecutionFailure {
                kind: ExecutionFailureKind::Failed,
                message: format!("executor-start-failed: {error}"),
            })?;
        let mut stdin = child.stdin.take().ok_or_else(|| ExecutionFailure {
            kind: ExecutionFailureKind::Failed,
            message: "executor-stdin-unavailable".into(),
        })?;
        stdin
            .write_all(request.prompt.as_bytes())
            .await
            .map_err(io_failure)?;
        drop(stdin);

        let deadline = tokio::time::sleep(Duration::from_secs(request.timeout_seconds));
        tokio::pin!(deadline);
        let status = tokio::select! {
            status = child.wait() => status.map_err(io_failure)?,
            _ = cancel.changed() => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                return Err(ExecutionFailure { kind: ExecutionFailureKind::Cancelled, message: "job cancelled".into() });
            }
            _ = &mut deadline => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                return Err(ExecutionFailure { kind: ExecutionFailureKind::TimedOut, message: format!("job exceeded {} seconds", request.timeout_seconds) });
            }
        };
        if status.success() {
            return Ok(ExecutionResult {
                exit_code: status.code().unwrap_or(0),
            });
        }
        let authentication_failed = stderr_indicates_auth_failure(&stderr_path);
        let detail = error_detail(&stderr_path, request.error_reporting);
        Err(ExecutionFailure {
            kind: if authentication_failed {
                ExecutionFailureKind::Authentication
            } else {
                ExecutionFailureKind::Failed
            },
            message: if detail.is_empty() {
                format!("executor exited with {}", status.code().unwrap_or(-1))
            } else {
                detail
            },
        })
    }
}

#[derive(Clone)]
pub struct WorkerService {
    inner: Arc<WorkerInner>,
}

struct WorkerInner {
    root: PathBuf,
    executor: Arc<dyn WorkerExecutor>,
    state: RwLock<WorkerState>,
    uploads: Mutex<HashMap<String, UploadState>>,
    cancellations: Mutex<HashMap<String, watch::Sender<bool>>>,
}

#[derive(Default)]
struct WorkerState {
    accounts: BTreeMap<String, WorkerAccount>,
    tabs: BTreeMap<String, WorkerTab>,
    jobs: BTreeMap<String, WorkerJob>,
    objects: BTreeMap<String, WorkerObject>,
    idempotency: HashMap<String, (String, String)>,
}

struct UploadState {
    name: String,
    path: PathBuf,
    total_bytes: u64,
    sha256: String,
}

impl WorkerService {
    pub fn process(root: impl AsRef<Path>) -> Result<Self> {
        let program = std::env::var_os("JAILGUN_CODEX_BIN")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("codex"));
        Self::new(root, Arc::new(ProcessExecutor::new(program)))
    }

    pub fn new(root: impl AsRef<Path>, executor: Arc<dyn WorkerExecutor>) -> Result<Self> {
        let root = root.as_ref().join("worker");
        for path in [
            root.clone(),
            root.join("accounts"),
            root.join("tabs"),
            root.join("objects"),
            root.join("uploads"),
            root.join("jobs"),
        ] {
            std::fs::create_dir_all(&path)?;
            restrict_directory(&path)?;
        }
        let mut state = WorkerState::default();
        for entry in std::fs::read_dir(root.join("accounts"))? {
            let path = entry?.path().join("account.json");
            if let Ok(bytes) = std::fs::read(&path) {
                if let Ok(mut account) = serde_json::from_slice::<WorkerAccount>(&bytes) {
                    account.status = WorkerAccountStatus::LoginRequired;
                    account.last_checked_ms = None;
                    account.active_tabs = 0;
                    state.accounts.insert(account.account_id.clone(), account);
                }
            }
        }
        for entry in std::fs::read_dir(root.join("objects"))? {
            let path = entry?.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                if let Ok(bytes) = std::fs::read(&path) {
                    if let Ok(object) = serde_json::from_slice::<WorkerObject>(&bytes) {
                        state.objects.insert(object.object_id.clone(), object);
                    }
                }
            }
        }
        Ok(Self {
            inner: Arc::new(WorkerInner {
                root,
                executor,
                state: RwLock::new(state),
                uploads: Mutex::new(HashMap::new()),
                cancellations: Mutex::new(HashMap::new()),
            }),
        })
    }

    pub async fn info(&self) -> WorkerInfo {
        WorkerInfo {
            interface_version: 2,
            executor: self.inner.executor.name().into(),
            model_aliases: model_aliases(),
            reasoning_efforts: ["low", "medium", "high", "xhigh", "max", "ultra"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            max_object_bytes: MAX_OBJECT_BYTES,
            max_chunk_bytes: MAX_CHUNK_BYTES,
            max_timeout_seconds: MAX_TIMEOUT_SECONDS,
            accounts: account_counts(self.inner.state.read().await.accounts.values()),
        }
    }

    pub async fn register_account(&self, request: AccountRegister) -> Result<WorkerAccount> {
        validate_id(&request.account_id, "account")?;
        let label = request
            .label
            .unwrap_or_else(|| request.account_id.clone())
            .trim()
            .to_string();
        if label.is_empty() || label.chars().count() > 120 || label.chars().any(char::is_control) {
            return Err(action(
                "invalid-account-label",
                "Account labels must contain 1–120 printable characters.",
                "Choose a short operator-facing label.",
            ));
        }
        {
            let state = self.inner.state.read().await;
            if state.accounts.contains_key(&request.account_id) {
                return Err(action(
                    "account-exists",
                    "An account with that ID is already registered.",
                    "Refresh the existing account or choose another account_id.",
                ));
            }
        }
        let account_root = self.account_root(&request.account_id);
        let codex_home = account_root.join("codex");
        std::fs::create_dir(&account_root)?;
        restrict_directory(&account_root)?;
        std::fs::create_dir(&codex_home)?;
        restrict_directory(&codex_home)?;
        let account = WorkerAccount {
            account_id: request.account_id.clone(),
            label,
            status: WorkerAccountStatus::LoginRequired,
            last_checked_ms: None,
            active_tabs: 0,
        };
        persist_account(&self.inner.root, &account)?;
        self.inner
            .state
            .write()
            .await
            .accounts
            .insert(request.account_id.clone(), account);
        self.refresh_account(AccountRef {
            account_id: request.account_id,
        })
        .await
    }

    pub async fn accounts(&self, refresh: bool) -> Result<WorkerAccounts> {
        if refresh {
            let account_ids = self
                .inner
                .state
                .read()
                .await
                .accounts
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            for account_id in account_ids {
                self.refresh_account(AccountRef { account_id }).await?;
            }
        }
        let accounts = self
            .inner
            .state
            .read()
            .await
            .accounts
            .values()
            .cloned()
            .collect::<Vec<_>>();
        Ok(WorkerAccounts {
            counts: account_counts(accounts.iter()),
            accounts,
        })
    }

    pub async fn refresh_account(&self, request: AccountRef) -> Result<WorkerAccount> {
        validate_id(&request.account_id, "account")?;
        {
            let state = self.inner.state.read().await;
            if !state.accounts.contains_key(&request.account_id) {
                return Err(not_found("account"));
            }
        }
        let status = self
            .inner
            .executor
            .auth_status(self.codex_home(&request.account_id))
            .await;
        let account = {
            let mut state = self.inner.state.write().await;
            let account = state
                .accounts
                .get_mut(&request.account_id)
                .ok_or_else(|| not_found("account"))?;
            account.status = status;
            account.last_checked_ms = Some(now_ms());
            account.clone()
        };
        persist_account(&self.inner.root, &account)?;
        Ok(account)
    }

    pub async fn tabs(&self) -> WorkerTabs {
        WorkerTabs {
            tabs: self
                .inner
                .state
                .read()
                .await
                .tabs
                .values()
                .cloned()
                .collect(),
        }
    }

    pub async fn open_tab(&self, request: TabOpen) -> Result<WorkerTab> {
        let account_id = self.resolve_account(request.account_id.as_deref()).await?;
        let tab_id = request
            .tab_id
            .unwrap_or_else(|| format!("tab-{}", uuid::Uuid::new_v4().simple()));
        validate_id(&tab_id, "tab")?;
        let model = normalize_model(&request.model)?;
        let workspace = self.inner.root.join("tabs").join(&tab_id);
        {
            let state = self.inner.state.read().await;
            if state.tabs.contains_key(&tab_id) {
                return Err(action(
                    "tab-exists",
                    "A tab with that ID already exists.",
                    "List tabs and choose a new tab_id.",
                ));
            }
        }
        std::fs::create_dir(&workspace).map_err(Error::Io)?;
        restrict_directory(&workspace)?;
        if let Some(object_id) = &request.input_object_id {
            let object = self.object(object_id).await?;
            let path = self.object_path(&object.object_id);
            if let Err(error) = extract_archive(&path, &workspace) {
                let _ = std::fs::remove_dir_all(&workspace);
                return Err(error);
            }
        }
        let tab = WorkerTab {
            tab_id: tab_id.clone(),
            account_id: account_id.clone(),
            status: "open".into(),
            model,
            reasoning_effort: request.reasoning_effort,
            active_job_id: None,
            input_object_id: request.input_object_id,
            created_ms: now_ms(),
        };
        let mut state = self.inner.state.write().await;
        state.tabs.insert(tab_id, tab.clone());
        if let Some(account) = state.accounts.get_mut(&account_id) {
            account.active_tabs += 1;
        }
        Ok(tab)
    }

    pub async fn close_tab(&self, request: TabRef) -> Result<WorkerTab> {
        let active = {
            let state = self.inner.state.read().await;
            let tab = state
                .tabs
                .get(&request.tab_id)
                .ok_or_else(|| not_found("tab"))?;
            tab.active_job_id.clone()
        };
        if let Some(job_id) = active {
            if !request.force {
                return Err(action(
                    "tab-busy",
                    "The tab has an active job.",
                    "Cancel the job or close the tab with force=true.",
                ));
            }
            let _ = self.cancel_job(JobRef { job_id }).await?;
        }
        let mut state = self.inner.state.write().await;
        let tab = state
            .tabs
            .get_mut(&request.tab_id)
            .ok_or_else(|| not_found("tab"))?;
        tab.status = "closed".into();
        let account_id = tab.account_id.clone();
        let tab = tab.clone();
        if let Some(account) = state.accounts.get_mut(&account_id) {
            account.active_tabs = account.active_tabs.saturating_sub(1);
        }
        Ok(tab)
    }

    pub async fn set_model(&self, request: TabModel) -> Result<WorkerTab> {
        let model = normalize_model(&request.model)?;
        let mut state = self.inner.state.write().await;
        let tab = state
            .tabs
            .get_mut(&request.tab_id)
            .ok_or_else(|| not_found("tab"))?;
        if tab.status != "open" || tab.active_job_id.is_some() {
            return Err(action(
                "tab-busy",
                "Model settings can change only on an idle open tab.",
                "Wait for or cancel the active job, then retry.",
            ));
        }
        tab.model = model;
        tab.reasoning_effort = request.reasoning_effort;
        Ok(tab.clone())
    }

    pub async fn submit_job(&self, request: JobSubmit) -> Result<WorkerJob> {
        validate_job(&request)?;
        let digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&request)?));
        {
            let state = self.inner.state.read().await;
            if let Some((stored_digest, job_id)) = state.idempotency.get(&request.idempotency_key) {
                if stored_digest != &digest {
                    return Err(action(
                        "idempotency-conflict",
                        "That idempotency key was already used with different job settings.",
                        "Reuse the original request or choose a new idempotency key.",
                    ));
                }
                return state
                    .jobs
                    .get(job_id)
                    .cloned()
                    .ok_or_else(|| not_found("job"));
            }
        }
        let tab_id = match request.tab_id.clone() {
            Some(tab_id) => {
                if request.input_object_id.is_some() {
                    return Err(action(
                        "input-object-with-existing-tab",
                        "An input object can be attached only while opening a new tab.",
                        "Open a tab with input_object_id, then submit the job to that tab.",
                    ));
                }
                tab_id
            }
            None => {
                self.open_tab(TabOpen {
                    tab_id: None,
                    account_id: request.account_id.clone(),
                    model: request.model.clone().unwrap_or_else(default_model),
                    reasoning_effort: request.reasoning_effort.unwrap_or_default(),
                    input_object_id: request.input_object_id.clone(),
                })
                .await?
                .tab_id
            }
        };
        let job_id = format!("job-{}", uuid::Uuid::new_v4().simple());
        let (account_id, model, reasoning_effort) = {
            let mut state = self.inner.state.write().await;
            let tab = state
                .tabs
                .get_mut(&tab_id)
                .ok_or_else(|| not_found("tab"))?;
            if tab.status != "open" || tab.active_job_id.is_some() {
                return Err(action(
                    "tab-busy",
                    "The selected tab is not idle and open.",
                    "Choose another tab or wait for the active job.",
                ));
            }
            let model = match &request.model {
                Some(model) => normalize_model(model)?,
                None => tab.model.clone(),
            };
            let effort = request.reasoning_effort.unwrap_or(tab.reasoning_effort);
            if let Some(requested_account) = &request.account_id {
                if requested_account != &tab.account_id {
                    return Err(action(
                        "account-tab-mismatch",
                        "The requested account does not own the selected tab.",
                        "Omit account_id or select a tab opened for that account.",
                    ));
                }
            }
            tab.model = model.clone();
            tab.reasoning_effort = effort;
            tab.active_job_id = Some(job_id.clone());
            (tab.account_id.clone(), model, effort)
        };
        let job = WorkerJob {
            job_id: job_id.clone(),
            tab_id: tab_id.clone(),
            account_id,
            status: "queued".into(),
            model,
            reasoning_effort,
            timeout_seconds: request.timeout_seconds,
            error_reporting: request.error_reporting,
            output_object_id: None,
            exit_code: None,
            error: None,
            created_ms: now_ms(),
            started_ms: None,
            finished_ms: None,
        };
        {
            let mut state = self.inner.state.write().await;
            state
                .idempotency
                .insert(request.idempotency_key.clone(), (digest, job_id.clone()));
            state.jobs.insert(job_id.clone(), job.clone());
        }
        let (cancel_tx, cancel_rx) = watch::channel(false);
        self.inner
            .cancellations
            .lock()
            .await
            .insert(job_id.clone(), cancel_tx);
        let worker = self.clone();
        tokio::spawn(async move {
            worker.run_job(job_id, request.prompt, cancel_rx).await;
        });
        Ok(job)
    }

    pub async fn job(&self, job_id: &str) -> Result<WorkerJob> {
        self.inner
            .state
            .read()
            .await
            .jobs
            .get(job_id)
            .cloned()
            .ok_or_else(|| not_found("job"))
    }

    pub async fn cancel_job(&self, request: JobRef) -> Result<WorkerJob> {
        let job = self.job(&request.job_id).await?;
        if is_terminal(&job.status) {
            return Ok(job);
        }
        if let Some(cancel) = self.inner.cancellations.lock().await.get(&request.job_id) {
            let _ = cancel.send(true);
        }
        let mut state = self.inner.state.write().await;
        let job = state
            .jobs
            .get_mut(&request.job_id)
            .ok_or_else(|| not_found("job"))?;
        job.status = "cancelling".into();
        Ok(job.clone())
    }

    pub async fn objects(&self) -> WorkerObjects {
        WorkerObjects {
            objects: self
                .inner
                .state
                .read()
                .await
                .objects
                .values()
                .cloned()
                .collect(),
        }
    }

    pub async fn put_object(&self, request: ObjectPut) -> Result<ObjectPutResult> {
        validate_object_request(&request)?;
        let bytes = BASE64.decode(&request.data_base64).map_err(|_| {
            action(
                "invalid-base64",
                "Object chunks must use standard base64.",
                "Encode the original tar.gz bytes without line wrapping.",
            )
        })?;
        if bytes.len() > MAX_CHUNK_BYTES {
            return Err(action(
                "chunk-too-large",
                format!("Decoded chunks may not exceed {MAX_CHUNK_BYTES} bytes."),
                "Send a smaller chunk and continue at next_offset.",
            ));
        }
        let upload_id = request
            .upload_id
            .clone()
            .unwrap_or_else(|| format!("upload-{}", uuid::Uuid::new_v4().simple()));
        validate_id(&upload_id, "upload")?;
        let path = self
            .inner
            .root
            .join("uploads")
            .join(format!("{upload_id}.part"));
        {
            let mut uploads = self.inner.uploads.lock().await;
            let upload = uploads
                .entry(upload_id.clone())
                .or_insert_with(|| UploadState {
                    name: request.name.clone(),
                    path: path.clone(),
                    total_bytes: request.total_bytes,
                    sha256: request.sha256.to_lowercase(),
                });
            if upload.name != request.name
                || upload.total_bytes != request.total_bytes
                || upload.sha256 != request.sha256.to_lowercase()
            {
                return Err(action(
                    "upload-metadata-conflict",
                    "Upload metadata changed between chunks.",
                    "Restart with a new upload_id and consistent metadata.",
                ));
            }
            let current = upload.path.metadata().map(|m| m.len()).unwrap_or(0);
            if current != request.offset {
                return Err(action(
                    "upload-offset-conflict",
                    format!("Expected offset {current}, received {}.", request.offset),
                    "Retry using the returned next_offset.",
                ));
            }
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&upload.path)?;
            file.write_all(&bytes)?;
            file.sync_data()?;
        }
        let next_offset = request.offset + bytes.len() as u64;
        if !request.final_chunk {
            return Ok(ObjectPutResult {
                upload_id,
                next_offset,
                committed: false,
                object: None,
            });
        }
        if next_offset != request.total_bytes {
            return Err(action(
                "upload-size-mismatch",
                format!(
                    "Expected {} bytes, received {next_offset}.",
                    request.total_bytes
                ),
                "Resume the upload or restart it with the correct total_bytes.",
            ));
        }
        let digest = sha256_file(&path)?;
        if digest != request.sha256.to_lowercase() {
            return Err(action(
                "upload-digest-mismatch",
                "The completed upload does not match sha256.",
                "Discard the upload and resend the original tar.gz bytes.",
            ));
        }
        jailgun_core::validate_tar_gz(&path, false).map_err(|error| {
            action(
                "unsafe-archive",
                error.to_string(),
                "Upload a non-empty tar.gz with safe relative paths and no .git entries.",
            )
        })?;
        let object_id = format!("object-{}", uuid::Uuid::new_v4().simple());
        let destination = self.object_path(&object_id);
        std::fs::rename(&path, &destination)?;
        let object = WorkerObject {
            object_id: object_id.clone(),
            name: request.name,
            kind: "input".into(),
            media_type: "application/gzip".into(),
            sha256: digest,
            size_bytes: next_offset,
            created_ms: now_ms(),
        };
        persist_object(&self.inner.root, &object)?;
        self.inner
            .state
            .write()
            .await
            .objects
            .insert(object_id, object.clone());
        self.inner.uploads.lock().await.remove(&upload_id);
        Ok(ObjectPutResult {
            upload_id,
            next_offset,
            committed: true,
            object: Some(object),
        })
    }

    pub async fn get_object(&self, request: ObjectGet) -> Result<ObjectChunk> {
        if request.limit == 0 || request.limit as usize > MAX_CHUNK_BYTES {
            return Err(action(
                "invalid-chunk-limit",
                format!("limit must be between 1 and {MAX_CHUNK_BYTES}."),
                "Use a bounded chunk size and follow next_offset.",
            ));
        }
        let object = self.object(&request.object_id).await?;
        if request.offset > object.size_bytes {
            return Err(action(
                "invalid-object-offset",
                "offset is beyond the end of the object.",
                "Begin at zero or use the previous next_offset.",
            ));
        }
        let mut file = File::open(self.object_path(&object.object_id))?;
        use std::io::{Seek, SeekFrom};
        file.seek(SeekFrom::Start(request.offset))?;
        let remaining = object.size_bytes - request.offset;
        let count = remaining.min(u64::from(request.limit)) as usize;
        let mut bytes = vec![0; count];
        file.read_exact(&mut bytes)?;
        let end = request.offset + count as u64;
        Ok(ObjectChunk {
            object,
            offset: request.offset,
            next_offset: (end < remaining + request.offset).then_some(end),
            eof: end == remaining + request.offset,
            data_base64: BASE64.encode(bytes),
        })
    }

    async fn run_job(&self, job_id: String, prompt: String, cancel: watch::Receiver<bool>) {
        let (tab_id, account_id, request) = {
            let mut state = self.inner.state.write().await;
            let Some(job) = state.jobs.get_mut(&job_id) else {
                return;
            };
            job.status = "running".into();
            job.started_ms = Some(now_ms());
            let tab_id = job.tab_id.clone();
            let account_id = job.account_id.clone();
            let request = ExecutionRequest {
                job_id: job_id.clone(),
                workspace: self.inner.root.join("tabs").join(&tab_id),
                codex_home: self.codex_home(&account_id),
                log_dir: self.inner.root.join("jobs").join(&job_id),
                prompt,
                model: job.model.clone(),
                reasoning_effort: job.reasoning_effort,
                timeout_seconds: job.timeout_seconds,
                error_reporting: job.error_reporting,
            };
            (tab_id, account_id, request)
        };
        let result = match tokio::time::timeout(
            Duration::from_secs(request.timeout_seconds),
            self.inner.executor.execute(request.clone(), cancel),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(ExecutionFailure {
                kind: ExecutionFailureKind::TimedOut,
                message: format!("job exceeded {} seconds", request.timeout_seconds),
            }),
        };
        let output = if result.is_ok() {
            self.package_output(&job_id, &request.workspace).await.ok()
        } else {
            None
        };
        let authentication_failed = matches!(
            &result,
            Err(ExecutionFailure {
                kind: ExecutionFailureKind::Authentication,
                ..
            })
        );
        let mut state = self.inner.state.write().await;
        if let Some(tab) = state.tabs.get_mut(&tab_id) {
            if tab.active_job_id.as_deref() == Some(&job_id) {
                tab.active_job_id = None;
            }
        }
        if let Some(job) = state.jobs.get_mut(&job_id) {
            job.finished_ms = Some(now_ms());
            match result {
                Ok(result) => {
                    job.status = "completed".into();
                    job.exit_code = Some(result.exit_code);
                    job.output_object_id = output.as_ref().map(|object| object.object_id.clone());
                    if output.is_none() {
                        job.status = "failed".into();
                        job.error = Some("output archive could not be created".into());
                    }
                }
                Err(error) => {
                    job.status = match error.kind {
                        ExecutionFailureKind::Cancelled => "cancelled",
                        ExecutionFailureKind::TimedOut => "timed-out",
                        ExecutionFailureKind::Authentication => "failed",
                        ExecutionFailureKind::Failed => "failed",
                    }
                    .into();
                    if job.error_reporting != ErrorReporting::Off {
                        job.error = Some(error.message);
                    }
                }
            }
        }
        if let Some(object) = output {
            state.objects.insert(object.object_id.clone(), object);
        }
        let account = if authentication_failed {
            state.accounts.get_mut(&account_id).map(|account| {
                account.status = WorkerAccountStatus::LoginRequired;
                account.last_checked_ms = Some(now_ms());
                account.clone()
            })
        } else {
            None
        };
        drop(state);
        if let Some(account) = account {
            let _ = persist_account(&self.inner.root, &account);
        }
        self.inner.cancellations.lock().await.remove(&job_id);
    }

    async fn package_output(&self, job_id: &str, workspace: &Path) -> Result<WorkerObject> {
        let object_id = format!("object-{}", uuid::Uuid::new_v4().simple());
        let path = self.object_path(&object_id);
        let workspace = workspace.to_path_buf();
        let output = path.clone();
        tokio::task::spawn_blocking(move || build_archive(&workspace, &output))
            .await
            .map_err(|_| {
                action(
                    "archive-task-failed",
                    "Output packaging stopped unexpectedly.",
                    "Inspect daemon logs and retry the job.",
                )
            })??;
        let size_bytes = path.metadata()?.len();
        let object = WorkerObject {
            object_id,
            name: format!("{job_id}.tar.gz"),
            kind: "output".into(),
            media_type: "application/gzip".into(),
            sha256: sha256_file(&path)?,
            size_bytes,
            created_ms: now_ms(),
        };
        persist_object(&self.inner.root, &object)?;
        Ok(object)
    }

    async fn object(&self, object_id: &str) -> Result<WorkerObject> {
        self.inner
            .state
            .read()
            .await
            .objects
            .get(object_id)
            .cloned()
            .ok_or_else(|| not_found("object"))
    }

    fn object_path(&self, object_id: &str) -> PathBuf {
        self.inner
            .root
            .join("objects")
            .join(format!("{object_id}.tar.gz"))
    }

    fn account_root(&self, account_id: &str) -> PathBuf {
        self.inner.root.join("accounts").join(account_id)
    }

    fn codex_home(&self, account_id: &str) -> PathBuf {
        self.account_root(account_id).join("codex")
    }

    async fn resolve_account(&self, requested: Option<&str>) -> Result<String> {
        if let Some(account_id) = requested {
            validate_id(account_id, "account")?;
            let account = self
                .refresh_account(AccountRef {
                    account_id: account_id.to_string(),
                })
                .await?;
            return match account.status {
                WorkerAccountStatus::Ready => Ok(account.account_id),
                WorkerAccountStatus::LoginRequired => Err(action(
                    "account-login-required",
                    "The selected account must be logged in again.",
                    "Log in through that account's private CODEX_HOME, then call account_refresh.",
                )),
                WorkerAccountStatus::Unavailable => Err(action(
                    "account-check-unavailable",
                    "The selected account could not be checked.",
                    "Verify the Codex executable, then call account_refresh.",
                )),
            };
        }
        let accounts = self.accounts(true).await?;
        let ready = accounts
            .accounts
            .into_iter()
            .filter(|account| account.status == WorkerAccountStatus::Ready)
            .map(|account| account.account_id)
            .collect::<Vec<_>>();
        match ready.as_slice() {
            [account_id] => Ok(account_id.clone()),
            [] => Err(action(
                "account-login-required",
                "No authenticated worker account is ready.",
                "Register and log in an account, then call account_refresh.",
            )),
            _ => Err(action(
                "account-required",
                "More than one authenticated account is ready.",
                "Set account_id when opening the tab or submitting an implicit-tab job.",
            )),
        }
    }
}

fn default_model() -> String {
    "gpt-6-astra".into()
}

fn default_timeout_seconds() -> u64 {
    DEFAULT_TIMEOUT_SECONDS
}

fn default_chunk_limit() -> u32 {
    64 * 1024
}

fn default_true() -> bool {
    true
}

fn account_counts<'a>(accounts: impl Iterator<Item = &'a WorkerAccount>) -> WorkerAccountCounts {
    let mut counts = WorkerAccountCounts::default();
    for account in accounts {
        counts.total += 1;
        match account.status {
            WorkerAccountStatus::Ready => counts.ready += 1,
            WorkerAccountStatus::LoginRequired => counts.login_required += 1,
            WorkerAccountStatus::Unavailable => counts.unavailable += 1,
        }
    }
    counts
}

fn model_aliases() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("astra".into(), "gpt-6-astra".into()),
        ("sol".into(), "gpt-5.6-sol".into()),
    ])
}

fn normalize_model(model: &str) -> Result<String> {
    let value = model.trim().to_lowercase();
    if let Some(model) = model_aliases().get(&value) {
        return Ok(model.clone());
    }
    if value.is_empty()
        || value.len() > 80
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
    {
        return Err(action(
            "invalid-model",
            "Model must be an alias such as astra/sol or a safe model identifier.",
            "Call jailgun.worker.info for aliases and supported efforts.",
        ));
    }
    Ok(value)
}

fn validate_job(request: &JobSubmit) -> Result<()> {
    let prompt_chars = request.prompt.chars().count();
    if prompt_chars == 0 || prompt_chars > MAX_PROMPT_CHARS {
        return Err(action(
            "invalid-prompt",
            format!("prompt must contain 1–{MAX_PROMPT_CHARS} characters."),
            "Send a bounded task prompt.",
        ));
    }
    if request.idempotency_key.is_empty() || request.idempotency_key.len() > 128 {
        return Err(action(
            "invalid-idempotency-key",
            "idempotency_key must contain 1–128 bytes.",
            "Use a stable unique key for this logical submission.",
        ));
    }
    if request.timeout_seconds == 0 || request.timeout_seconds > MAX_TIMEOUT_SECONDS {
        return Err(action(
            "invalid-timeout",
            format!("timeout_seconds must be between 1 and {MAX_TIMEOUT_SECONDS}."),
            "Choose a bounded timeout.",
        ));
    }
    if let Some(tab_id) = &request.tab_id {
        validate_id(tab_id, "tab")?;
    }
    Ok(())
}

fn validate_object_request(request: &ObjectPut) -> Result<()> {
    if request.name.is_empty()
        || request.name.len() > 160
        || !request.name.ends_with(".tar.gz")
        || Path::new(&request.name)
            .file_name()
            .and_then(|v| v.to_str())
            != Some(&request.name)
    {
        return Err(action(
            "invalid-object-name",
            "Object names must be safe .tar.gz basenames.",
            "Use a name such as source.tar.gz.",
        ));
    }
    if request.total_bytes == 0 || request.total_bytes > MAX_OBJECT_BYTES {
        return Err(action(
            "invalid-object-size",
            format!("total_bytes must be between 1 and {MAX_OBJECT_BYTES}."),
            "Reduce the archive size before uploading.",
        ));
    }
    if request.sha256.len() != 64
        || !request
            .sha256
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(action(
            "invalid-sha256",
            "sha256 must be a 64-character hexadecimal digest.",
            "Hash the complete tar.gz bytes and retry.",
        ));
    }
    if request.offset > request.total_bytes {
        return Err(action(
            "invalid-upload-offset",
            "offset exceeds total_bytes.",
            "Restart at offset zero or use the returned next_offset.",
        ));
    }
    Ok(())
}

fn validate_id(value: &str, kind: &'static str) -> Result<()> {
    if value.is_empty()
        || value.len() > 96
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
    {
        return Err(action(
            "invalid-id",
            format!("{kind}_id contains unsafe characters."),
            "Use 1–96 ASCII letters, digits, hyphens, or underscores.",
        ));
    }
    Ok(())
}

fn is_terminal(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled" | "timed-out")
}

fn action(code: &'static str, message: impl Into<String>, next_action: &'static str) -> Error {
    Error::action(code, message, next_action)
}

fn not_found(kind: &'static str) -> Error {
    action(
        "not-found",
        format!("The requested {kind} does not exist."),
        "List the available worker resources and retry with an existing ID.",
    )
}

fn io_failure(error: std::io::Error) -> ExecutionFailure {
    ExecutionFailure {
        kind: ExecutionFailureKind::Failed,
        message: format!("executor-io-failed: {error}"),
    }
}

fn error_detail(path: &Path, mode: ErrorReporting) -> String {
    if mode == ErrorReporting::Off {
        return String::new();
    }
    let Ok(mut bytes) = std::fs::read(path) else {
        return String::new();
    };
    let limit = match mode {
        ErrorReporting::Off => 0,
        ErrorReporting::Summary => 4 * 1024,
        ErrorReporting::Detailed => 64 * 1024,
    };
    if bytes.len() > limit {
        bytes = bytes.split_off(bytes.len() - limit);
    }
    String::from_utf8_lossy(&bytes).trim().to_string()
}

fn stderr_indicates_auth_failure(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let text = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
    [
        "not logged in",
        "login required",
        "authentication required",
        "unauthorized",
        "status 401",
        "http 401",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

fn restrict_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn persist_object(root: &Path, object: &WorkerObject) -> Result<()> {
    let path = root
        .join("objects")
        .join(format!("{}.json", object.object_id));
    let staging_path = path.with_extension("json.tmp");
    std::fs::write(&staging_path, serde_json::to_vec_pretty(object)?)?;
    std::fs::rename(staging_path, path)?;
    Ok(())
}

fn persist_account(root: &Path, account: &WorkerAccount) -> Result<()> {
    let path = root
        .join("accounts")
        .join(&account.account_id)
        .join("account.json");
    let staging_path = path.with_extension("json.tmp");
    std::fs::write(&staging_path, serde_json::to_vec_pretty(account)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staging_path, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(staging_path, path)?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn safe_archive_path(path: &Path) -> Result<PathBuf> {
    let mut safe = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) if value != ".git" => safe.push(value),
            Component::CurDir => {}
            _ => {
                return Err(action(
                    "unsafe-archive",
                    "Archive contains an unsafe path.",
                    "Remove absolute paths, parent traversal, links, and .git entries.",
                ))
            }
        }
    }
    if safe.as_os_str().is_empty() {
        return Err(action(
            "unsafe-archive",
            "Archive contains an empty path.",
            "Rebuild the tar.gz with safe relative paths.",
        ));
    }
    Ok(safe)
}

fn extract_archive(path: &Path, workspace: &Path) -> Result<()> {
    let file = File::open(path)?;
    let mut archive = Archive::new(GzDecoder::new(file));
    let mut expanded = 0_u64;
    for entry in archive.entries().map_err(Error::Io)? {
        let mut entry = entry.map_err(Error::Io)?;
        let kind = entry.header().entry_type();
        if !kind.is_file() && !kind.is_dir() {
            return Err(action(
                "unsafe-archive",
                "Archives may contain only regular files and directories.",
                "Remove links, devices, and special entries.",
            ));
        }
        let relative = safe_archive_path(&entry.path().map_err(Error::Io)?)?;
        expanded = expanded.saturating_add(entry.header().size().unwrap_or(0));
        if expanded > MAX_OBJECT_BYTES {
            return Err(action(
                "archive-expanded-too-large",
                "Expanded archive exceeds the worker limit.",
                "Reduce the archive before uploading.",
            ));
        }
        let destination = workspace.join(relative);
        if kind.is_dir() {
            std::fs::create_dir_all(destination)?;
        } else {
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut output = File::create(destination)?;
            std::io::copy(&mut entry, &mut output)?;
        }
    }
    Ok(())
}

fn build_archive(workspace: &Path, output: &Path) -> Result<()> {
    let file = File::create(output)?;
    let encoder = GzEncoder::new(file, Compression::fast());
    let mut archive = Builder::new(encoder);
    let mut total = 0_u64;
    append_directory(&mut archive, workspace, workspace, &mut total)?;
    archive.finish()?;
    Ok(())
}

fn append_directory<W: Write>(
    archive: &mut Builder<W>,
    root: &Path,
    directory: &Path,
    total: &mut u64,
) -> Result<()> {
    let mut entries = std::fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let relative = path.strip_prefix(root).map_err(|_| {
            action(
                "archive-path-failed",
                "Output path escaped its tab workspace.",
                "Inspect the tab workspace and retry.",
            )
        })?;
        if relative
            .components()
            .any(|component| component.as_os_str() == ".git")
        {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            archive.append_dir(relative, &path)?;
            append_directory(archive, root, &path, total)?;
        } else if metadata.is_file() {
            *total = total.saturating_add(metadata.len());
            if *total > MAX_OBJECT_BYTES {
                return Err(action(
                    "output-too-large",
                    "Output files exceed the worker object limit.",
                    "Remove build products or split the result into smaller jobs.",
                ));
            }
            archive.append_path_with_name(&path, relative)?;
        }
    }
    Ok(())
}

fn now_ms() -> i64 {
    (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockExecutor;

    #[async_trait]
    impl WorkerExecutor for MockExecutor {
        fn name(&self) -> &'static str {
            "mock"
        }

        async fn auth_status(&self, codex_home: PathBuf) -> WorkerAccountStatus {
            if codex_home
                .components()
                .any(|component| component.as_os_str() == "expired")
            {
                WorkerAccountStatus::LoginRequired
            } else {
                WorkerAccountStatus::Ready
            }
        }

        async fn execute(
            &self,
            request: ExecutionRequest,
            _cancel: watch::Receiver<bool>,
        ) -> std::result::Result<ExecutionResult, ExecutionFailure> {
            std::fs::write(
                request.workspace.join("jailgun-result.md"),
                format!("# Mock result\n\n{}", request.prompt),
            )
            .map_err(io_failure)?;
            Ok(ExecutionResult { exit_code: 0 })
        }
    }

    struct BlockingExecutor;

    #[async_trait]
    impl WorkerExecutor for BlockingExecutor {
        fn name(&self) -> &'static str {
            "blocking-mock"
        }

        async fn execute(
            &self,
            _request: ExecutionRequest,
            mut cancel: watch::Receiver<bool>,
        ) -> std::result::Result<ExecutionResult, ExecutionFailure> {
            let _ = cancel.changed().await;
            Err(ExecutionFailure {
                kind: ExecutionFailureKind::Cancelled,
                message: "mock cancellation observed".into(),
            })
        }
    }

    struct SlowExecutor;

    #[async_trait]
    impl WorkerExecutor for SlowExecutor {
        fn name(&self) -> &'static str {
            "slow-mock"
        }

        async fn execute(
            &self,
            _request: ExecutionRequest,
            _cancel: watch::Receiver<bool>,
        ) -> std::result::Result<ExecutionResult, ExecutionFailure> {
            std::future::pending().await
        }
    }

    fn service(temp: &tempfile::TempDir) -> WorkerService {
        WorkerService::new(temp.path(), Arc::new(MockExecutor)).unwrap()
    }

    async fn register_default(service: &WorkerService) {
        let account = service
            .register_account(AccountRegister {
                account_id: "primary".into(),
                label: Some("Primary test account".into()),
            })
            .await
            .unwrap();
        assert_eq!(account.status, WorkerAccountStatus::Ready);
    }

    async fn wait_terminal(service: &WorkerService, job_id: &str) -> WorkerJob {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let job = service.job(job_id).await.unwrap();
                if is_terminal(&job.status) {
                    return job;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn mock_job_uses_alias_effort_and_returns_verified_tar() {
        let temp = tempfile::tempdir().unwrap();
        let service = service(&temp);
        register_default(&service).await;
        let tab = service
            .open_tab(TabOpen {
                tab_id: Some("dream".into()),
                account_id: Some("primary".into()),
                model: "sol".into(),
                reasoning_effort: ReasoningEffort::Xhigh,
                input_object_id: None,
            })
            .await
            .unwrap();
        assert_eq!(tab.model, "gpt-5.6-sol");
        let request = JobSubmit {
            prompt: "Build the fixture".into(),
            tab_id: Some(tab.tab_id),
            account_id: None,
            model: Some("astra".into()),
            reasoning_effort: Some(ReasoningEffort::Ultra),
            timeout_seconds: 60,
            error_reporting: ErrorReporting::Detailed,
            input_object_id: None,
            idempotency_key: "fixture-1".into(),
        };
        let accepted = service.submit_job(request.clone()).await.unwrap();
        assert_eq!(
            service.submit_job(request).await.unwrap().job_id,
            accepted.job_id
        );
        let job = wait_terminal(&service, &accepted.job_id).await;
        assert_eq!(job.status, "completed");
        assert_eq!(job.model, "gpt-6-astra");
        assert_eq!(job.reasoning_effort, ReasoningEffort::Ultra);
        let object = service
            .object(job.output_object_id.as_deref().unwrap())
            .await
            .unwrap();
        assert_eq!(
            sha256_file(&service.object_path(&object.object_id)).unwrap(),
            object.sha256
        );
        jailgun_core::validate_tar_gz(service.object_path(&object.object_id), false).unwrap();
    }

    #[tokio::test]
    async fn chunked_objects_round_trip_and_extract_safely() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("hello.txt"), "hello").unwrap();
        let archive = temp.path().join("source.tar.gz");
        build_archive(&source, &archive).unwrap();
        let bytes = std::fs::read(&archive).unwrap();
        let digest = format!("{:x}", Sha256::digest(&bytes));
        let service = service(&temp);
        register_default(&service).await;
        let midpoint = bytes.len() / 2;
        let first = service
            .put_object(ObjectPut {
                upload_id: None,
                name: "source.tar.gz".into(),
                offset: 0,
                data_base64: BASE64.encode(&bytes[..midpoint]),
                final_chunk: false,
                total_bytes: bytes.len() as u64,
                sha256: digest.clone(),
            })
            .await
            .unwrap();
        let final_chunk = service
            .put_object(ObjectPut {
                upload_id: Some(first.upload_id),
                name: "source.tar.gz".into(),
                offset: midpoint as u64,
                data_base64: BASE64.encode(&bytes[midpoint..]),
                final_chunk: true,
                total_bytes: bytes.len() as u64,
                sha256: digest,
            })
            .await
            .unwrap();
        let object = final_chunk.object.unwrap();
        let tab = service
            .open_tab(TabOpen {
                tab_id: Some("from-object".into()),
                account_id: Some("primary".into()),
                model: "astra".into(),
                reasoning_effort: ReasoningEffort::Medium,
                input_object_id: Some(object.object_id.clone()),
            })
            .await
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(
                service
                    .inner
                    .root
                    .join("tabs")
                    .join(tab.tab_id)
                    .join("hello.txt")
            )
            .unwrap(),
            "hello"
        );
        let chunk = service
            .get_object(ObjectGet {
                object_id: object.object_id,
                offset: 0,
                limit: MAX_CHUNK_BYTES as u32,
            })
            .await
            .unwrap();
        assert_eq!(BASE64.decode(chunk.data_base64).unwrap(), bytes);
        assert!(chunk.eof);
    }

    #[tokio::test]
    async fn busy_tabs_and_idempotency_conflicts_fail_closed() {
        let temp = tempfile::tempdir().unwrap();
        let service = service(&temp);
        register_default(&service).await;
        let request = JobSubmit {
            prompt: "first".into(),
            tab_id: None,
            account_id: None,
            model: None,
            reasoning_effort: None,
            timeout_seconds: 60,
            error_reporting: ErrorReporting::Off,
            input_object_id: None,
            idempotency_key: "same".into(),
        };
        let accepted = service.submit_job(request.clone()).await.unwrap();
        let mut changed = request;
        changed.prompt = "different".into();
        assert_eq!(
            service.submit_job(changed).await.unwrap_err().code(),
            "idempotency-conflict"
        );
        let _ = wait_terminal(&service, &accepted.job_id).await;
        let closed = service
            .close_tab(TabRef {
                tab_id: accepted.tab_id,
                force: false,
            })
            .await
            .unwrap();
        assert_eq!(closed.status, "closed");
    }

    #[tokio::test]
    async fn mock_cancellation_and_timeout_reach_distinct_terminal_states() {
        let cancelled_root = tempfile::tempdir().unwrap();
        let cancelled =
            WorkerService::new(cancelled_root.path(), Arc::new(BlockingExecutor)).unwrap();
        register_default(&cancelled).await;
        let request = JobSubmit {
            prompt: "wait".into(),
            tab_id: None,
            account_id: None,
            model: Some("sol".into()),
            reasoning_effort: Some(ReasoningEffort::Medium),
            timeout_seconds: 30,
            error_reporting: ErrorReporting::Summary,
            input_object_id: None,
            idempotency_key: "cancel-fixture".into(),
        };
        let job = cancelled.submit_job(request).await.unwrap();
        cancelled
            .cancel_job(JobRef {
                job_id: job.job_id.clone(),
            })
            .await
            .unwrap();
        let job = wait_terminal(&cancelled, &job.job_id).await;
        assert_eq!(job.status, "cancelled");
        assert_eq!(job.error.as_deref(), Some("mock cancellation observed"));

        let timeout_root = tempfile::tempdir().unwrap();
        let timeout = WorkerService::new(timeout_root.path(), Arc::new(SlowExecutor)).unwrap();
        register_default(&timeout).await;
        let job = timeout
            .submit_job(JobSubmit {
                prompt: "wait forever".into(),
                tab_id: None,
                account_id: None,
                model: None,
                reasoning_effort: None,
                timeout_seconds: 1,
                error_reporting: ErrorReporting::Off,
                input_object_id: None,
                idempotency_key: "timeout-fixture".into(),
            })
            .await
            .unwrap();
        let job = wait_terminal(&timeout, &job.job_id).await;
        assert_eq!(job.status, "timed-out");
        assert!(job.error.is_none());
    }

    #[tokio::test]
    async fn multiple_accounts_require_explicit_routing_and_stay_private() {
        let temp = tempfile::tempdir().unwrap();
        let service = service(&temp);
        for account_id in ["first", "second"] {
            service
                .register_account(AccountRegister {
                    account_id: account_id.into(),
                    label: None,
                })
                .await
                .unwrap();
        }
        let expired = service
            .register_account(AccountRegister {
                account_id: "expired".into(),
                label: Some("Needs login".into()),
            })
            .await
            .unwrap();
        assert_eq!(expired.status, WorkerAccountStatus::LoginRequired);
        let accounts = service.accounts(true).await.unwrap();
        assert_eq!(accounts.counts.total, 3);
        assert_eq!(accounts.counts.ready, 2);
        assert_eq!(accounts.counts.login_required, 1);
        let error = service
            .open_tab(TabOpen {
                tab_id: Some("expired-tab".into()),
                account_id: Some("expired".into()),
                model: "astra".into(),
                reasoning_effort: ReasoningEffort::Medium,
                input_object_id: None,
            })
            .await
            .unwrap_err();
        assert_eq!(error.code(), "account-login-required");
        let error = service
            .open_tab(TabOpen {
                tab_id: Some("ambiguous".into()),
                account_id: None,
                model: "astra".into(),
                reasoning_effort: ReasoningEffort::High,
                input_object_id: None,
            })
            .await
            .unwrap_err();
        assert_eq!(error.code(), "account-required");
        let tab = service
            .open_tab(TabOpen {
                tab_id: Some("routed".into()),
                account_id: Some("second".into()),
                model: "sol".into(),
                reasoning_effort: ReasoningEffort::Xhigh,
                input_object_id: None,
            })
            .await
            .unwrap();
        assert_eq!(tab.account_id, "second");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(service.account_root("second").join("account.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn authentication_failures_are_detected_without_returning_credentials() {
        let temp = tempfile::tempdir().unwrap();
        let stderr = temp.path().join("stderr.log");
        std::fs::write(&stderr, "request failed: HTTP 401 unauthorized\n").unwrap();
        assert!(stderr_indicates_auth_failure(&stderr));
        std::fs::write(&stderr, "ordinary model failure\n").unwrap();
        assert!(!stderr_indicates_auth_failure(&stderr));
    }
}

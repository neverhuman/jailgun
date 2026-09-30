use super::*;

#[derive(Clone)]
pub struct WorkerService {
    pub(super) inner: Arc<WorkerInner>,
}

pub(super) struct WorkerInner {
    pub(super) root: PathBuf,
    pub(super) executor: Arc<dyn WorkerExecutor>,
    pub(super) state: RwLock<WorkerState>,
    pub(super) uploads: Mutex<HashMap<String, UploadState>>,
    pub(super) cancellations: Mutex<HashMap<String, watch::Sender<bool>>>,
}

#[derive(Default)]
pub(super) struct WorkerState {
    pub(super) accounts: BTreeMap<String, WorkerAccount>,
    pub(super) tabs: BTreeMap<String, WorkerTab>,
    pub(super) jobs: BTreeMap<String, WorkerJob>,
    pub(super) objects: BTreeMap<String, WorkerObject>,
    pub(super) idempotency: HashMap<String, (String, String)>,
}

pub(super) struct UploadState {
    pub(super) name: String,
    pub(super) path: PathBuf,
    pub(super) total_bytes: u64,
    pub(super) sha256: String,
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

    pub(super) fn account_root(&self, account_id: &str) -> PathBuf {
        self.inner.root.join("accounts").join(account_id)
    }

    pub(super) fn codex_home(&self, account_id: &str) -> PathBuf {
        self.account_root(account_id).join("codex")
    }

    pub(super) async fn resolve_account(&self, requested: Option<&str>) -> Result<String> {
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

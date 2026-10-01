use super::*;

#[derive(Clone)]
pub struct WorkerService {
    pub(super) inner: Arc<WorkerInner>,
}

pub(super) struct WorkerInner {
    pub(super) root: PathBuf,
    pub(super) executor: Arc<dyn WorkerExecutor>,
    pub(super) account_store: Option<jailgun_workflow::Store>,
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
    pub fn browser(
        root: impl AsRef<Path>,
        supervisor: jailgun_orchestrator::concept::AccountSupervisor,
    ) -> Result<Self> {
        let account_store = supervisor.store.clone();
        Self::build(
            root,
            Arc::new(BrowserExecutor::new(supervisor)),
            Some(account_store),
        )
    }

    pub fn new(root: impl AsRef<Path>, executor: Arc<dyn WorkerExecutor>) -> Result<Self> {
        Self::build(root, executor, None)
    }

    fn build(
        root: impl AsRef<Path>,
        executor: Arc<dyn WorkerExecutor>,
        account_store: Option<jailgun_workflow::Store>,
    ) -> Result<Self> {
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
                account_store,
                state: RwLock::new(state),
                uploads: Mutex::new(HashMap::new()),
                cancellations: Mutex::new(HashMap::new()),
            }),
        })
    }

    pub async fn info(&self) -> WorkerInfo {
        if self.inner.account_store.is_some() {
            let _ = self.sync_browser_accounts().await;
        }
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
        if self.inner.account_store.is_some() {
            return Err(action(
                "dashboard-account-required",
                "Browser accounts are registered only through the operator dashboard.",
                "Open Accounts → Connect ChatGPT, complete normal website login, then call account_list.",
            ));
        }
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
        std::fs::create_dir(&account_root)?;
        restrict_directory(&account_root)?;
        let account = WorkerAccount {
            account_id: request.account_id.clone(),
            label,
            status: WorkerAccountStatus::LoginRequired,
            last_checked_ms: None,
            active_tabs: 0,
            selected_model: None,
            available_models: Vec::new(),
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
        if self.inner.account_store.is_some() {
            self.sync_browser_accounts().await?;
        }
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
            let mut accounts = Vec::with_capacity(account_ids.len());
            for account_id in account_ids {
                accounts.push(self.refresh_account(AccountRef { account_id }).await?);
            }
            return Ok(WorkerAccounts {
                counts: account_counts(accounts.iter()),
                accounts,
            });
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
        if self.inner.account_store.is_some() {
            let status = self.inner.executor.auth_status(&request.account_id).await;
            self.sync_browser_accounts().await?;
            let mut state = self.inner.state.write().await;
            let account = state
                .accounts
                .get_mut(&request.account_id)
                .ok_or_else(|| not_found("account"))?;
            if status == WorkerAccountStatus::Unavailable {
                account.status = WorkerAccountStatus::Unavailable;
            }
            return Ok(account.clone());
        }
        {
            let state = self.inner.state.read().await;
            if !state.accounts.contains_key(&request.account_id) {
                return Err(not_found("account"));
            }
        }
        let status = self.inner.executor.auth_status(&request.account_id).await;
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
                    "Reconnect the account through the Jailgun dashboard, then call account_refresh.",
                )),
                WorkerAccountStatus::Unavailable => Err(action(
                    "account-check-unavailable",
                    "The selected account could not be checked.",
                    "Inspect the supervised browser and reconnect through the Jailgun dashboard.",
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
                "Connect and verify an account in the Jailgun dashboard, then call account_refresh.",
            )),
            _ => Err(action(
                "account-required",
                "More than one authenticated account is ready.",
                "Set account_id when opening the tab or submitting an implicit-tab job.",
            )),
        }
    }

    async fn sync_browser_accounts(&self) -> Result<()> {
        let Some(store) = self.inner.account_store.as_ref() else {
            return Ok(());
        };
        let readiness = store.accounts().await?;
        let sessions = store.account_sessions().await?;
        let mut state = self.inner.state.write().await;
        let open_tabs = state.tabs.values().filter(|tab| tab.status == "open").fold(
            BTreeMap::<String, usize>::new(),
            |mut counts, tab| {
                *counts.entry(tab.account_id.clone()).or_default() += 1;
                counts
            },
        );
        let mut accounts = BTreeMap::new();
        for account in readiness {
            let session = sessions
                .iter()
                .find(|session| session.account_id == account.id);
            let observation = session.and_then(|session| session.observation.as_ref());
            let status = match account.readiness.as_str() {
                "ready" => WorkerAccountStatus::Ready,
                "login-required" => WorkerAccountStatus::LoginRequired,
                _ => WorkerAccountStatus::Unavailable,
            };
            let worker_account = WorkerAccount {
                account_id: account.id.clone(),
                label: session
                    .map(|session| session.email.clone())
                    .unwrap_or_else(|| account.id.clone()),
                status,
                last_checked_ms: Some(now_ms()),
                active_tabs: open_tabs.get(&account.id).copied().unwrap_or(0),
                selected_model: observation.map(|observation| observation.model.clone()),
                available_models: observation
                    .map(|observation| observation.available_models.clone())
                    .unwrap_or_default(),
            };
            accounts.insert(account.id, worker_account);
        }
        state.accounts = accounts;
        Ok(())
    }
}

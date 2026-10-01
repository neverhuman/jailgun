use crate::{
    database::{invalid_transition, not_found},
    model::*,
    Error, Result, Store,
};
use jailgun_core::{BrowserAccount, BrowserAccountRoots, BrowserProfileRegistry, ProviderIdentity};
use rusqlite::{params, OptionalExtension};

impl Store {
    pub async fn account_metadata(&self) -> Result<Vec<BrowserAccount>> {
        self.call(|db| {
            let mut statement = db
                .connection
                .prepare("SELECT metadata_json FROM accounts ORDER BY id")?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
        })
        .await
    }

    /// The database writer serializes allocation across every account registration.
    pub async fn connect_account(&self, request: ConnectAccount) -> Result<BrowserAccount> {
        self.call(move |db| {
            if request.email.len() > 254
                || !request.email.contains('@')
                || request.email.chars().any(char::is_whitespace)
            {
                return Err(Error::action(
                    "invalid-account",
                    "A valid account email is required.",
                    "Enter the email used to sign in to ChatGPT.",
                ));
            }
            let tx = db.connection.transaction()?;
            let mut registry = BrowserProfileRegistry::default();
            {
                let mut statement = tx.prepare("SELECT metadata_json FROM accounts")?;
                for row in statement.query_map([], |row| row.get::<_, String>(0))? {
                    registry.accounts.push(serde_json::from_str(&row?)?);
                }
            }
            let id = request
                .id
                .unwrap_or_else(|| jailgun_core::default_account_id(&request.email));
            if let Some(account) = registry.account(&id) {
                if !account.email_hint.eq_ignore_ascii_case(&request.email) {
                    return Err(invalid_transition());
                }
                return Ok(account.clone());
            }
            if registry
                .accounts
                .iter()
                .any(|account| account.email_hint.eq_ignore_ascii_case(&request.email))
            {
                return Err(invalid_transition());
            }
            let roots = BrowserAccountRoots {
                profile_root: db.root.join("profiles"),
                state_root: db.root.join("browser-state"),
                downloads_root: db.root.join("downloads"),
            };
            let (port, _listener) = (9224..=u16::MAX)
                .filter(|port| !registry.accounts.iter().any(|a| a.cdp_port == *port))
                .find_map(|port| {
                    std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
                        .ok()
                        .map(|listener| (port, listener))
                })
                .ok_or_else(|| {
                    Error::action(
                        "port-unavailable",
                        "No account port allocation is available.",
                        "Inspect the local listeners.",
                    )
                })?;
            let account = registry
                .upsert_account(&request.email, Some(id), &roots, port, 10)
                .map_err(|_| {
                    Error::action(
                        "invalid-account",
                        "The account ID or profile metadata is invalid.",
                        "Use a unique account ID containing letters, digits and hyphens.",
                    )
                })?;
            crate::accounts::insert_account(&tx, &account)?;
            claim_profile(&account.profile_dir, &db.root)?;
            tx.commit()?;
            Ok(account)
        })
        .await
    }

    pub async fn account_sessions(&self) -> Result<Vec<AccountSession>> {
        self.call(|db| {
            let mut statement = db.connection.prepare("SELECT s.account_id,a.email_hint,s.lifecycle,s.model_json,s.observation_json,s.observed_ms,s.login_expires_ms,s.error_code FROM account_sessions s JOIN accounts a ON a.id=s.account_id ORDER BY s.account_id")?;
            let rows = statement.query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,Option<String>>(3)?,row.get::<_,Option<String>>(4)?,row.get(5)?,row.get(6)?,row.get(7)?)))?;
            rows.map(|row| { let (account_id,email,lifecycle,model,observation,observed_ms,login_expires_ms,error_code)=row?;
                Ok(AccountSession {account_id,email,lifecycle,model:model.map(|s| serde_json::from_str(&s)).transpose()?,observation:observation.map(|s| serde_json::from_str(&s)).transpose()?,observed_ms,login_expires_ms,error_code,login_view_available:false}) }).collect()
        }).await
    }

    pub async fn login_state(
        &self,
        id: String,
        state: String,
        error: Option<String>,
        expires: Option<i64>,
    ) -> Result<()> {
        if ![
            "disconnected",
            "starting",
            "login-required",
            "waiting-for-user",
            "verifying",
            "ready",
            "expired",
            "mismatch",
            "failed",
            "cancelled",
        ]
        .contains(&state.as_str())
        {
            return Err(invalid_transition());
        }
        self.call(move |db| {
            let current:String=db.connection.query_row("SELECT lifecycle FROM account_sessions WHERE account_id=?1",[&id],|r|r.get(0)).optional()?.ok_or_else(not_found)?;
            if current=="cancelled" && !["starting","cancelled"].contains(&state.as_str()) {
                return Err(Error::action("account-login-cancelled","This login session was cancelled.","Reconnect to start another login session."));
            }
            if db.connection.execute("UPDATE account_sessions SET lifecycle=?2,error_code=?3,login_expires_ms=?4,observation_json=CASE WHEN ?2='starting' THEN NULL ELSE observation_json END,observed_ms=CASE WHEN ?2='starting' THEN NULL ELSE observed_ms END WHERE account_id=?1",params![id,state,error,expires])? == 0 { return Err(not_found()); }
            Ok(())
        }).await
    }

    pub async fn observe_account(
        &self,
        id: String,
        observation: AccountObservation,
        now: i64,
    ) -> Result<bool> {
        self.call(move |db| {
            let metadata: String = db.connection.query_row("SELECT metadata_json FROM accounts WHERE id=?1",[&id],|row| row.get(0)).optional()?.ok_or_else(not_found)?;
            let mut account: BrowserAccount = serde_json::from_str(&metadata)?;
            let bound = account.provider_account_id.is_some();
            let identity = ProviderIdentity {id:observation.identity.id.clone(),email:observation.identity.email.clone()};
            if account.confirm_identity(&identity, now.to_string()).is_err() { return Err(Error::action("account-mismatch", "The detected account does not match this profile's binding.", "Sign in to the registered account.")); }
            if observation.model.trim().is_empty() || observation.available_models.len()>100 || observation.available_models.iter().any(|model| model.len()>200) { return Err(invalid_transition()); }
            db.connection.execute("UPDATE account_sessions SET observation_json=?2,observed_ms=?3,error_code=NULL WHERE account_id=?1", params![id,serde_json::to_string(&observation)?,now])?;
            Ok(bound)
        }).await
    }

    /// Confirmation uses a recent observed identity; it cannot bind arbitrary caller-supplied data.
    pub async fn confirm_account(
        &self,
        id: String,
        request: ConfirmAccount,
        now: i64,
    ) -> Result<()> {
        self.call(move |db| {
            let tx=db.connection.transaction()?;
            let (observation, observed, lifecycle): (Option<String>,Option<i64>,String) = tx.query_row("SELECT observation_json,observed_ms,lifecycle FROM account_sessions WHERE account_id=?1",[&id],|row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?.ok_or_else(not_found)?;
            if !["verifying","ready"].contains(&lifecycle.as_str()) || observed.is_none_or(|time| time>now || now-time>30_000) { return Err(Error::action("account-verification-expired", "A recent browser identity observation is required.", "Reconnect and confirm the detected account.")); }
            let observation: AccountObservation = serde_json::from_str(&observation.ok_or_else(invalid_transition)?)?;
            if observation.identity.id != request.identity.id || !observation.identity.email.eq_ignore_ascii_case(&request.identity.email) { return Err(invalid_transition()); }
            if let ModelSelection::Specific {name} = &request.model { if name!=&observation.model && !observation.available_models.contains(name) { return Err(Error::action("model-unavailable", "The selected model was not observed on this account.", "Choose an available model or the account's current selection.")); } }
            let (metadata,cooldown):(String,i64)=tx.query_row("SELECT metadata_json,cooldown_until_ms FROM accounts WHERE id=?1",[&id],|r|Ok((r.get(0)?,r.get(1)?)))?;
            if now<cooldown { return Err(Error::action("account-cooldown","The provider cooldown has not elapsed.","Wait until the indicated retry time.")); }
            let mut account:BrowserAccount=serde_json::from_str(&metadata)?;
            let identity=ProviderIdentity{id:observation.identity.id,email:observation.identity.email};
            account.confirm_identity(&identity,now.to_string()).map_err(|_|invalid_transition())?;
            tx.execute("UPDATE accounts SET provider_id=?2,metadata_json=?3,readiness='ready',cooldown_until_ms=0,rate_limit_count=0 WHERE id=?1",params![id,identity.id,serde_json::to_string(&account)?])?;
            tx.execute("UPDATE account_sessions SET model_json=?2,lifecycle='ready',error_code=NULL,login_expires_ms=NULL WHERE account_id=?1",params![id,serde_json::to_string(&request.model)?])?;
            crate::accounts::resume_verified_runs(&tx,&id,now)?;
            tx.commit()?;
            Ok(())
        }).await
    }

    pub async fn account_model(&self, id: String) -> Result<ModelSelection> {
        self.call(move |db| read_account_model(&db.connection, &id))
            .await
    }
}

/// A marker can outlive a rolled-back database transaction. Reuse only the
/// exact same installation's regular marker; never replace foreign ownership.
fn claim_profile(profile: &std::path::Path, root: &std::path::Path) -> Result<()> {
    jailgun_core::browser_registry::profile_ownership::claim(profile, root).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            Error::action(
                "profile-ownership-conflict",
                "The browser profile has a different or invalid installation owner.",
                "Inspect the profile and installation metadata; do not delete an active owner's marker.",
            )
        } else {
            error.into()
        }
    })
}

pub(crate) fn read_account_model(
    connection: &rusqlite::Connection,
    id: &str,
) -> Result<ModelSelection> {
    let model: Option<String> = connection
        .query_row(
            "SELECT model_json FROM account_sessions WHERE account_id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(not_found)?;
    Ok(serde_json::from_str(&model.ok_or_else(|| {
        Error::action(
            "model-selection-required",
            "Confirm the account identity and model before submitting.",
            "Open Accounts in the operator dashboard.",
        )
    })?)?)
}

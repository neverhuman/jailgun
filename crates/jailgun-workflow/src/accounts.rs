use crate::{
    database::{atomic_new, event, hash, not_found},
    model::AccountReadiness,
    Error, Result, Store,
};
use jailgun_core::{BrowserAccount, BrowserAccountStatus, BrowserProfileRegistry};
use rusqlite::{params, OptionalExtension};
use std::{fs, path::PathBuf};

impl Store {
    /// The caller must first stop the old scheduler. Profiles are referenced in place.
    pub async fn import_registry(&self, path: PathBuf, now: i64) -> Result<usize> {
        self.call(move |db| {
            let bytes = fs::read(&path)?;
            let registry: BrowserProfileRegistry = serde_json::from_slice(&bytes)?;
            if registry.version != 1 {
                return Err(Error::action(
                    "migration-format",
                    "Unsupported account registry version.",
                    "Use the matching importer and preserve the source registry.",
                ));
            }
            let digest = hash(&bytes);
            let imported: bool = db.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM imports WHERE source_sha256=?1)",
                [&digest],
                |r| r.get(0),
            )?;
            if imported {
                return Ok(0);
            }
            let tx = db.connection.transaction()?;
            for account in &registry.accounts {
                insert_account(&tx, account)?;
            }
            // Unique content-addressed backups never overwrite an earlier migration input.
            let backup = db
                .root
                .join(format!("account-registry-{digest}.json.backup"));
            if backup.exists() {
                if hash(&fs::read(&backup)?) != digest {
                    return Err(Error::action(
                        "backup-conflict",
                        "An account migration backup has conflicting content.",
                        "Preserve the conflicting files and inspect the runtime.",
                    ));
                }
            } else {
                atomic_new(&backup, &bytes)?;
            }
            tx.execute("INSERT INTO imports VALUES(?1,?2)", params![digest, now])?;
            tx.commit()?;
            Ok(registry.accounts.len())
        })
        .await
    }

    pub async fn accounts(&self) -> Result<Vec<AccountReadiness>> {
        self.call(|db| {
            let mut stmt = db.connection.prepare("SELECT a.id,a.readiness,a.capacity,(SELECT count(*) FROM reservations r WHERE r.account_id=a.id),a.next_submit_ms,a.cooldown_until_ms,a.rate_limit_count FROM accounts a ORDER BY a.id")?;
            let rows = stmt.query_map([], |r| Ok(AccountReadiness { id:r.get(0)?, readiness:r.get(1)?, capacity:r.get(2)?, active:r.get(3)?, next_submit_ms:r.get(4)?, cooldown_until_ms:r.get(5)?, rate_limit_count:r.get(6)? }))?;
            Ok(rows.collect::<std::result::Result<Vec<_>,_>>()?)
        }).await
    }

    /// A verified browser identity is required, including after daemon restart.
    pub async fn verify_account(
        &self,
        id: String,
        identity: jailgun_core::ProviderIdentity,
        now: i64,
        operator_action: bool,
    ) -> Result<()> {
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            let data: Option<(String,String,i64)> = tx.query_row("SELECT metadata_json,readiness,cooldown_until_ms FROM accounts WHERE id=?1", [&id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
            let (metadata, readiness, cooldown) = data.ok_or_else(not_found)?;
            let cancelled:bool=tx.query_row("SELECT lifecycle='cancelled' FROM account_sessions WHERE account_id=?1",[&id],|r|r.get(0))?;
            if cancelled { return Err(Error::action("account-login-cancelled","This login session was cancelled.","Reconnect to start another login session.")); }
            if now < cooldown || (readiness == "paused" && !operator_action) {
                return Err(Error::action("account-cooldown", "The account is waiting for its cooldown or operator action.", "Wait for the indicated retry time, then verify the account."));
            }
            let mut account: BrowserAccount = serde_json::from_str(&metadata)?;
            if account.confirm_identity(&identity, now.to_string()).is_err() {
                tx.execute("UPDATE accounts SET readiness='mismatch' WHERE id=?1", [&id])?;
                pause_account_runs(&tx, &id, "account-mismatch", now)?;
                tx.commit()?;
                return Err(Error::action("account-mismatch", "The browser identity differs from the registered account.", "Reconnect the correct account through the operator dashboard."));
            }
            tx.execute("UPDATE accounts SET provider_id=?2,metadata_json=?3,readiness='ready',cooldown_until_ms=0,rate_limit_count=CASE WHEN ?4 THEN 0 ELSE rate_limit_count END WHERE id=?1", params![id,identity.id,serde_json::to_string(&account)?,operator_action])?;
            resume_verified_runs(&tx,&id,now)?;
            tx.commit()?;
            Ok(())
        }).await
    }

    pub async fn account_expired(&self, id: String, now: i64) -> Result<()> {
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            if tx.execute("UPDATE accounts SET readiness='expired' WHERE id=?1", [&id])? == 0 {
                return Err(not_found());
            }
            pause_account_runs(&tx, &id, "authentication-expired", now)?;
            tx.commit()?;
            Ok(())
        })
        .await
    }

    pub async fn rate_limited(&self, id: String, retry_at: Option<i64>, now: i64) -> Result<()> {
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            let until = retry_at.unwrap_or(now.saturating_add(300_000)).max(now);
            if tx.execute("UPDATE accounts SET rate_limit_count=rate_limit_count+1,cooldown_until_ms=max(cooldown_until_ms,?2),readiness=CASE WHEN rate_limit_count>=1 THEN 'paused' ELSE 'cooldown' END WHERE id=?1", params![id,until])? == 0 { return Err(not_found()); }
            let ids = run_ids(&tx, &id, "running")?.into_iter().chain(run_ids(&tx, &id, "queued")?);
            for run in ids { event(&tx, &run, "account-rate-limited", serde_json::json!({"retry_at_ms":until}), now)?; }
            tx.commit()?;
            Ok(())
        }).await
    }
}

pub(crate) fn insert_account(
    tx: &rusqlite::Transaction<'_>,
    account: &BrowserAccount,
) -> Result<()> {
    if jailgun_core::validate_account_id(&account.id).is_err()
        || account.email_hint.trim().is_empty()
        || !account.profile_dir.is_absolute()
        || account.cdp_port == 0
        || account.max_tabs == 0
    {
        return Err(Error::action(
            "migration-account-invalid",
            "The registry contains invalid account metadata.",
            "Correct the source metadata without moving or replacing the profile.",
        ));
    }
    let existing: Option<String> = tx
        .query_row(
            "SELECT metadata_json FROM accounts WHERE id=?1",
            [&account.id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(existing) = existing {
        let existing: BrowserAccount = serde_json::from_str(&existing)?;
        if existing
            .email_hint
            .eq_ignore_ascii_case(&account.email_hint)
            && existing.profile_dir == account.profile_dir
            && existing.cdp_port == account.cdp_port
            && existing.provider_account_id == account.provider_account_id
        {
            return Ok(());
        }
        return Err(Error::action(
            "migration-identity-conflict",
            "An account ID already refers to different identity or profile metadata.",
            "Resolve the conflict explicitly; no account was replaced.",
        ));
    }
    let readiness =
        if account.status == BrowserAccountStatus::Ready && account.provider_account_id.is_some() {
            "ready"
        } else {
            "login-required"
        };
    tx.execute("INSERT INTO accounts(id,email_hint,provider_id,profile_dir,metadata_json,cdp_port,capacity,readiness) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![account.id,account.email_hint,account.provider_account_id,account.profile_dir.to_string_lossy(),serde_json::to_string(account)?,account.cdp_port,account.max_tabs,readiness])?;
    Ok(())
}

fn run_ids(tx: &rusqlite::Transaction<'_>, account: &str, status: &str) -> Result<Vec<String>> {
    let mut stmt =
        tx.prepare("SELECT id FROM runs WHERE account_id=?1 AND status=?2 ORDER BY created_ms,id")?;
    let rows = stmt.query_map(params![account, status], |r| r.get(0))?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

fn pause_account_runs(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
    reason: &str,
    now: i64,
) -> Result<()> {
    let ids = run_ids(tx, id, "running")?
        .into_iter()
        .chain(run_ids(tx, id, "queued")?);
    for run in ids {
        tx.execute(
            "UPDATE runs SET status='waiting-for-auth',pause_reason=?2 WHERE id=?1",
            params![run, reason],
        )?;
        event(
            tx,
            &run,
            "account-login-required",
            serde_json::json!({"account_id":id,"reason":reason}),
            now,
        )?;
    }
    Ok(())
}

pub(crate) fn resume_verified_runs(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
    now: i64,
) -> Result<()> {
    for run in run_ids(tx, id, "waiting-for-auth")? {
        if tx.execute(
            "UPDATE runs SET status='queued',pause_reason=NULL WHERE id=?1 AND deadline_ms>?2",
            params![run, now],
        )? > 0
        {
            event(
                tx,
                &run,
                "account-verified",
                serde_json::json!({"account_id":id}),
                now,
            )?;
        }
    }
    Ok(())
}

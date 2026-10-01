use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{
    account::{BrowserAccount, BrowserAccountRoots, BrowserAccountStatus},
    ids::{default_account_id, validate_account_id},
    storage::{atomic_write, default_registry_path, ensure_private_dir, private_file},
    BrowserRegistryError,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserProfileRegistry {
    #[serde(default = "default_registry_version")]
    pub version: u16,
    #[serde(default)]
    pub accounts: Vec<BrowserAccount>,
}

impl Default for BrowserProfileRegistry {
    fn default() -> Self {
        Self {
            version: default_registry_version(),
            accounts: Vec::new(),
        }
    }
}

impl BrowserProfileRegistry {
    pub fn default_path_from_env(env_name: &str) -> PathBuf {
        env::var_os(env_name)
            .map(PathBuf::from)
            .unwrap_or_else(default_registry_path)
    }

    pub fn load_or_default(path: &Path) -> Result<Self, BrowserRegistryError> {
        match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(|source| BrowserRegistryError::Parse {
                path: path.display().to_string(),
                source,
            }),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(BrowserRegistryError::Read {
                path: path.display().to_string(),
                source,
            }),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), BrowserRegistryError> {
        if let Some(parent) = path.parent() {
            ensure_private_dir(parent)?;
        }
        let bytes =
            serde_json::to_vec_pretty(self).map_err(|source| BrowserRegistryError::Write {
                path: path.display().to_string(),
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, source),
            })?;
        atomic_write(path, &bytes)
    }

    /// The lock covers load, mutation and atomic replacement, across processes.
    pub fn update<T>(
        path: &Path,
        update: impl FnOnce(&mut Self) -> Result<T, BrowserRegistryError>,
    ) -> Result<T, BrowserRegistryError> {
        if let Some(parent) = path.parent() {
            ensure_private_dir(parent)?;
        }
        let lock_path = path.with_extension("registry.lock");
        let lock = private_file(&lock_path, false)?;
        lock.lock().map_err(|source| BrowserRegistryError::Lock {
            path: lock_path.display().to_string(),
            source,
        })?;
        let mut registry = Self::load_or_default(path)?;
        let result = update(&mut registry)?;
        registry.save(path)?;
        Ok(result)
    }

    pub fn register_account(
        path: &Path,
        email_hint: &str,
        id: Option<String>,
        roots: &BrowserAccountRoots,
        port_start: u16,
        max_tabs: u16,
    ) -> Result<BrowserAccount, BrowserRegistryError> {
        Self::update(path, |registry| {
            let account_id = id.unwrap_or_else(|| default_account_id(email_hint));
            if registry.account(&account_id).is_some() {
                return registry.upsert_account(
                    email_hint,
                    Some(account_id),
                    roots,
                    port_start,
                    max_tabs,
                );
            }
            for port in port_start.max(1024)..=u16::MAX {
                if registry.accounts.iter().any(|a| a.cdp_port == port) {
                    continue;
                }
                // Keep the probe bound until the account is allocated under the registry lock.
                if let Ok(_listener) =
                    std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
                {
                    return registry.upsert_account(
                        email_hint,
                        Some(account_id),
                        roots,
                        port,
                        max_tabs,
                    );
                }
            }
            Err(BrowserRegistryError::NoAvailablePort)
        })
    }

    pub fn verify_account(
        path: &Path,
        id: &str,
        identity: &super::ProviderIdentity,
        verified_at: String,
    ) -> Result<BrowserAccount, BrowserRegistryError> {
        Self::update(path, |registry| {
            let account = registry
                .account_mut(id)
                .ok_or_else(|| BrowserRegistryError::MissingAccount(id.into()))?;
            match account.confirm_identity(identity, verified_at) {
                Ok(()) => Ok(Ok(account.clone())),
                Err(error) => {
                    account.status = BrowserAccountStatus::Degraded;
                    account.last_verified_at = None;
                    Ok(Err(error))
                }
            }
        })?
    }

    pub fn account(&self, id: &str) -> Option<&BrowserAccount> {
        self.accounts.iter().find(|account| account.id == id)
    }

    pub fn account_mut(&mut self, id: &str) -> Option<&mut BrowserAccount> {
        self.accounts.iter_mut().find(|account| account.id == id)
    }

    pub fn require_account(&self, id: &str) -> Result<&BrowserAccount, BrowserRegistryError> {
        self.account(id)
            .ok_or_else(|| BrowserRegistryError::MissingAccount(id.to_string()))
    }

    pub fn upsert_account(
        &mut self,
        email_hint: &str,
        id: Option<String>,
        roots: &BrowserAccountRoots,
        cdp_port: u16,
        max_tabs: u16,
    ) -> Result<BrowserAccount, BrowserRegistryError> {
        let id = id.unwrap_or_else(|| default_account_id(email_hint));
        validate_account_id(&id)?;
        if let Some(existing) = self.account(&id) {
            if !existing.email_hint.eq_ignore_ascii_case(email_hint.trim()) {
                return Err(BrowserRegistryError::AccountIdentityConflict(id));
            }
            existing.ensure_runtime_dirs()?;
            return Ok(existing.clone());
        }
        if cdp_port == 0
            || self
                .accounts
                .iter()
                .any(|account| account.cdp_port == cdp_port)
        {
            return Err(BrowserRegistryError::PortUnavailable(cdp_port));
        }
        let account = BrowserAccount {
            id: id.clone(),
            email_hint: email_hint.trim().to_string(),
            profile_dir: roots.profile_root.join(&id),
            state_dir: roots.state_root.join(&id),
            downloads_dir: roots.downloads_root.join(&id),
            cdp_port,
            max_tabs: max_tabs.max(1),
            status: BrowserAccountStatus::AuthRequired,
            last_verified_at: None,
            provider_account_id: None,
        };
        account.ensure_runtime_dirs()?;

        self.accounts.push(account.clone());
        Ok(account)
    }

    pub fn ready_accounts<'a>(
        &'a self,
        ids: &[String],
    ) -> Result<Vec<&'a BrowserAccount>, BrowserRegistryError> {
        let mut accounts = Vec::with_capacity(ids.len());
        for id in ids {
            let account = self.require_account(id)?;
            account.require_ready()?;
            accounts.push(account);
        }
        Ok(accounts)
    }
}

fn default_registry_version() -> u16 {
    1
}

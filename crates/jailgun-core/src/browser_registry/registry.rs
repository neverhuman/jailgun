use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{
    account::{BrowserAccount, BrowserAccountRoots, BrowserAccountStatus},
    ids::{default_account_id, validate_account_id},
    storage::{
        default_registry_path, ensure_private_dir, registry_tmp_path, set_private_file_permissions,
    },
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
        let tmp_path = registry_tmp_path(path);
        fs::write(&tmp_path, bytes).map_err(|source| BrowserRegistryError::Write {
            path: tmp_path.display().to_string(),
            source,
        })?;
        set_private_file_permissions(&tmp_path)?;
        fs::rename(&tmp_path, path).map_err(|source| BrowserRegistryError::Write {
            path: path.display().to_string(),
            source,
        })?;
        set_private_file_permissions(path)?;
        Ok(())
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
        };
        account.ensure_runtime_dirs()?;

        if let Some(existing) = self.account_mut(&id) {
            let last_verified_at = existing.last_verified_at.clone();
            let status = existing.status;
            *existing = BrowserAccount {
                status,
                last_verified_at,
                ..account.clone()
            };
        } else {
            self.accounts.push(account.clone());
        }
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

use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use super::{
    registry::BrowserProfileRegistry,
    storage::{atomic_write, ensure_private_dir, private_file},
    BrowserAccount, BrowserAccountStatus, BrowserRegistryError,
};

mod helpers;
mod lock;

#[cfg(test)]
mod tests;

use lock::InterprocessFileLock;

pub const DEFAULT_BROWSER_QUEUE_TIMEOUT_SECONDS: u64 = 30 * 60;
pub const MAX_BROWSER_QUEUE_TIMEOUT_SECONDS: u64 = 6 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserLeaseRequest {
    pub run_id: String,
    pub account_ids: Vec<String>,
    pub tabs: u16,
    pub allow_queueing: bool,
    pub queue_timeout_seconds: u64,
    pub lease_ttl_seconds: u64,
}

impl BrowserLeaseRequest {
    pub fn effective_queue_timeout_seconds(&self) -> u64 {
        self.queue_timeout_seconds
            .clamp(1, MAX_BROWSER_QUEUE_TIMEOUT_SECONDS)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserLeaseAllocation {
    pub account_id: String,
    pub tabs: u16,
}

#[derive(Debug)]
pub struct BrowserLease {
    lease_path: PathBuf,
    lease_ids: Vec<String>,
    accounts: Vec<BrowserAccount>,
    tab_accounts: Vec<BrowserAccount>,
    allocations: Vec<BrowserLeaseAllocation>,
    released: bool,
}

impl BrowserLease {
    pub fn accounts(&self) -> &[BrowserAccount] {
        &self.accounts
    }

    pub fn tab_accounts(&self) -> &[BrowserAccount] {
        &self.tab_accounts
    }

    pub fn allocations(&self) -> &[BrowserLeaseAllocation] {
        &self.allocations
    }

    pub fn release(&mut self) -> Result<(), BrowserRegistryError> {
        if self.released {
            return Ok(());
        }
        release_lease_ids(&self.lease_path, &self.lease_ids)?;
        self.released = true;
        Ok(())
    }
}

impl Drop for BrowserLease {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

#[derive(Debug, Clone)]
pub struct BrowserLeaseManager {
    registry_path: PathBuf,
    lease_path: PathBuf,
}

impl BrowserLeaseManager {
    /// Permanently route migrated profiles away from the archive scheduler.
    /// The same lease lock makes migration atomic with respect to new archive reservations.
    pub fn claim_workflow_profiles(&self, runtime: &Path) -> Result<(), BrowserRegistryError> {
        with_locked_leases(&self.lease_path, |state| {
            state.purge_stale(unix_seconds());
            if !state.leases.is_empty() {
                return Err(BrowserRegistryError::LeaseInvalid(
                    "migration-browser-busy: stop archive work before migrating accounts".into(),
                ));
            }
            let registry = BrowserProfileRegistry::load_or_default(&self.registry_path)?;
            for account in registry.accounts {
                account.ensure_runtime_dirs()?;
                super::profile_ownership::claim(&account.profile_dir, runtime).map_err(|source| {
                    if source.kind() == io::ErrorKind::AlreadyExists {
                        BrowserRegistryError::LeaseInvalid("migration-profile-conflict: another or invalid owner claims this profile".into())
                    } else {
                        BrowserRegistryError::Write { path: account.profile_dir.display().to_string(), source }
                    }
                })?;
            }
            Ok(())
        })
    }
    pub fn new(registry_path: impl Into<PathBuf>) -> Self {
        let registry_path = registry_path.into();
        let lease_path = lease_path_for_registry(&registry_path);
        Self {
            registry_path,
            lease_path,
        }
    }

    pub fn lease_path(&self) -> &Path {
        &self.lease_path
    }

    pub fn try_acquire(
        &self,
        request: &BrowserLeaseRequest,
    ) -> Result<BrowserLease, BrowserRegistryError> {
        self.try_acquire_at(request, unix_seconds())
    }

    fn try_acquire_at(
        &self,
        request: &BrowserLeaseRequest,
        now: u64,
    ) -> Result<BrowserLease, BrowserRegistryError> {
        if request.tabs == 0 {
            return Err(BrowserRegistryError::LeaseInvalid(
                "requested browser lease tabs must be positive".into(),
            ));
        }
        with_locked_leases(&self.lease_path, |state| {
            let registry = BrowserProfileRegistry::load_or_default(&self.registry_path)?;
            let accounts = ready_candidate_accounts(&registry, &request.account_ids)?;
            for account in &accounts {
                account.require_archive_access()?;
            }
            ensure_total_capacity(&accounts, request.tabs)?;
            state.purge_stale(now);
            let Some(plan) = allocate_tabs(&accounts, &state.leases, request.tabs) else {
                if request.allow_queueing {
                    return Err(BrowserRegistryError::LeaseBusy {
                        requested: request.tabs,
                    });
                }
                return Err(BrowserRegistryError::LeaseUnavailable {
                    requested: request.tabs,
                });
            };
            let owner_pid = std::process::id();
            let lease_ids = plan
                .allocations
                .iter()
                .map(|_| uuid::Uuid::new_v4().to_string())
                .collect::<Vec<_>>();
            let expires_at_epoch_secs = now.saturating_add(request.lease_ttl_seconds.max(60));
            for (lease_id, allocation) in lease_ids.iter().zip(plan.allocations.iter()) {
                state.leases.push(BrowserLeaseRecord {
                    lease_id: lease_id.clone(),
                    run_id: request.run_id.clone(),
                    account_id: allocation.account_id.clone(),
                    tabs: allocation.tabs,
                    owner_pid,
                    started_at_epoch_secs: now,
                    expires_at_epoch_secs,
                });
            }
            let unique_accounts = unique_accounts_for_allocations(&accounts, &plan.allocations);
            let tab_accounts = tab_accounts_for_plan(&accounts, &plan.tab_account_ids);
            Ok(BrowserLease {
                lease_path: self.lease_path.clone(),
                lease_ids,
                accounts: unique_accounts,
                tab_accounts,
                allocations: plan.allocations,
                released: false,
            })
        })
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct BrowserLeaseState {
    #[serde(default = "default_lease_state_version")]
    version: u16,
    #[serde(default)]
    leases: Vec<BrowserLeaseRecord>,
}

impl BrowserLeaseState {
    fn purge_stale(&mut self, now: u64) {
        self.leases.retain(|lease| {
            lease.expires_at_epoch_secs > now
                && owner_process_is_alive(lease.owner_pid)
                && lease.started_at_epoch_secs <= now.saturating_add(60)
        });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct BrowserLeaseRecord {
    lease_id: String,
    run_id: String,
    account_id: String,
    tabs: u16,
    owner_pid: u32,
    started_at_epoch_secs: u64,
    expires_at_epoch_secs: u64,
}

struct LeasePlan {
    allocations: Vec<BrowserLeaseAllocation>,
    tab_account_ids: Vec<String>,
}

use helpers::*;

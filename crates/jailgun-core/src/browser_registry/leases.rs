use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use super::{
    registry::BrowserProfileRegistry,
    storage::{ensure_private_dir, registry_tmp_path, set_private_file_permissions},
    BrowserAccount, BrowserAccountStatus, BrowserRegistryError,
};

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
        if request.tabs == 0 {
            return Err(BrowserRegistryError::LeaseInvalid(
                "requested browser lease tabs must be positive".into(),
            ));
        }
        let now = unix_seconds();
        with_locked_leases(&self.lease_path, |state| {
            let registry = BrowserProfileRegistry::load_or_default(&self.registry_path)?;
            let accounts = ready_candidate_accounts(&registry, &request.account_ids)?;
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
                .enumerate()
                .map(|(index, allocation)| {
                    format!("{}-{}-{}-{}", owner_pid, now, index, allocation.account_id)
                })
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

fn ready_candidate_accounts(
    registry: &BrowserProfileRegistry,
    requested_ids: &[String],
) -> Result<Vec<BrowserAccount>, BrowserRegistryError> {
    let mut accounts = Vec::new();
    if requested_ids.is_empty() {
        accounts.extend(
            registry
                .accounts
                .iter()
                .filter(|account| account.status == BrowserAccountStatus::Ready)
                .cloned(),
        );
        if accounts.is_empty() {
            return Err(BrowserRegistryError::NoReadyAccounts);
        }
        return Ok(accounts);
    }

    let mut seen = BTreeSet::new();
    for id in requested_ids {
        super::validate_account_id(id)?;
        if !seen.insert(id.clone()) {
            return Err(BrowserRegistryError::DuplicateAccountId(id.clone()));
        }
        let account = registry.require_account(id)?;
        account.require_ready()?;
        accounts.push(account.clone());
    }
    Ok(accounts)
}

fn ensure_total_capacity(
    accounts: &[BrowserAccount],
    requested_tabs: u16,
) -> Result<(), BrowserRegistryError> {
    let capacity = total_capacity(accounts);
    if requested_tabs > capacity {
        return Err(BrowserRegistryError::InsufficientAccountCapacity {
            requested: requested_tabs,
            capacity,
        });
    }
    Ok(())
}

fn allocate_tabs(
    accounts: &[BrowserAccount],
    active: &[BrowserLeaseRecord],
    requested_tabs: u16,
) -> Option<LeasePlan> {
    let account_ids = accounts
        .iter()
        .map(|account| account.id.clone())
        .collect::<BTreeSet<_>>();
    let mut used = active_loads(active, &account_ids);
    let mut allocated = BTreeMap::<String, u16>::new();
    let mut tab_account_ids = Vec::with_capacity(requested_tabs as usize);
    for _ in 0..requested_tabs {
        let account = accounts
            .iter()
            .filter(|account| used.get(&account.id).copied().unwrap_or(0) < account.max_tabs.max(1))
            .min_by_key(|account| {
                let current = used.get(&account.id).copied().unwrap_or(0);
                let weighted = (current as u32) * 1000 / account.max_tabs.max(1) as u32;
                (weighted, current, account.id.as_str())
            })?;
        *used.entry(account.id.clone()).or_default() += 1;
        *allocated.entry(account.id.clone()).or_default() += 1;
        tab_account_ids.push(account.id.clone());
    }
    Some(LeasePlan {
        allocations: allocated
            .into_iter()
            .map(|(account_id, tabs)| BrowserLeaseAllocation { account_id, tabs })
            .collect(),
        tab_account_ids,
    })
}

fn active_loads(
    active: &[BrowserLeaseRecord],
    account_ids: &BTreeSet<String>,
) -> BTreeMap<String, u16> {
    let mut loads = BTreeMap::new();
    for lease in active {
        if account_ids.contains(&lease.account_id) {
            *loads.entry(lease.account_id.clone()).or_default() += lease.tabs;
        }
    }
    loads
}

fn unique_accounts_for_allocations(
    accounts: &[BrowserAccount],
    allocations: &[BrowserLeaseAllocation],
) -> Vec<BrowserAccount> {
    let by_id = accounts
        .iter()
        .map(|account| (account.id.as_str(), account))
        .collect::<BTreeMap<_, _>>();
    let mut expanded = Vec::new();
    for allocation in allocations {
        if let Some(account) = by_id.get(allocation.account_id.as_str()) {
            expanded.push((*account).clone());
        }
    }
    expanded
}

fn tab_accounts_for_plan(
    accounts: &[BrowserAccount],
    tab_account_ids: &[String],
) -> Vec<BrowserAccount> {
    let by_id = accounts
        .iter()
        .map(|account| (account.id.as_str(), account))
        .collect::<BTreeMap<_, _>>();
    tab_account_ids
        .iter()
        .filter_map(|id| by_id.get(id.as_str()).map(|account| (*account).clone()))
        .collect()
}

fn total_capacity(accounts: &[BrowserAccount]) -> u16 {
    accounts
        .iter()
        .map(|account| account.max_tabs.max(1))
        .fold(0u16, u16::saturating_add)
}

fn release_lease_ids(lease_path: &Path, lease_ids: &[String]) -> Result<(), BrowserRegistryError> {
    let wanted = lease_ids.iter().cloned().collect::<BTreeSet<_>>();
    with_locked_leases(lease_path, |state| {
        let now = unix_seconds();
        state.purge_stale(now);
        state
            .leases
            .retain(|lease| !wanted.contains(&lease.lease_id));
        Ok(())
    })
}

fn with_locked_leases<T>(
    lease_path: &Path,
    update: impl FnOnce(&mut BrowserLeaseState) -> Result<T, BrowserRegistryError>,
) -> Result<T, BrowserRegistryError> {
    if let Some(parent) = lease_path.parent() {
        ensure_private_dir(parent)?;
    }
    let lock_path = lock_path_for_lease_path(lease_path);
    let lock_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|source| BrowserRegistryError::Lock {
            path: lock_path.display().to_string(),
            source,
        })?;
    set_private_file_permissions(&lock_path)?;
    let _guard = InterprocessFileLock::lock(lock_file, &lock_path)?;

    let mut state = load_lease_state(lease_path)?;
    let result = update(&mut state)?;
    save_lease_state(lease_path, &state)?;
    Ok(result)
}

fn load_lease_state(path: &Path) -> Result<BrowserLeaseState, BrowserRegistryError> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|source| BrowserRegistryError::Parse {
            path: path.display().to_string(),
            source,
        }),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(BrowserLeaseState::default()),
        Err(source) => Err(BrowserRegistryError::Read {
            path: path.display().to_string(),
            source,
        }),
    }
}

fn save_lease_state(path: &Path, state: &BrowserLeaseState) -> Result<(), BrowserRegistryError> {
    let bytes = serde_json::to_vec_pretty(state).map_err(|source| BrowserRegistryError::Write {
        path: path.display().to_string(),
        source: io::Error::new(io::ErrorKind::InvalidData, source),
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

fn lease_path_for_registry(registry_path: &Path) -> PathBuf {
    let file_name = registry_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("browser-profiles.json");
    registry_path.with_file_name(format!("{file_name}.leases.json"))
}

fn lock_path_for_lease_path(lease_path: &Path) -> PathBuf {
    let file_name = lease_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("browser-profiles.json.leases.json");
    lease_path.with_file_name(format!(".{file_name}.lock"))
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs()
}

fn default_lease_state_version() -> u16 {
    1
}

#[cfg(target_os = "linux")]
fn owner_process_is_alive(pid: u32) -> bool {
    pid == std::process::id() || PathBuf::from(format!("/proc/{pid}")).exists()
}

#[cfg(not(target_os = "linux"))]
fn owner_process_is_alive(_pid: u32) -> bool {
    true
}

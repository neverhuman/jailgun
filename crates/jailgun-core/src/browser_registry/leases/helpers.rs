use super::*;

pub(super) fn ready_candidate_accounts(
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
        super::super::validate_account_id(id)?;
        if !seen.insert(id.clone()) {
            return Err(BrowserRegistryError::DuplicateAccountId(id.clone()));
        }
        let account = registry.require_account(id)?;
        account.require_ready()?;
        accounts.push(account.clone());
    }
    Ok(accounts)
}

pub(super) fn ensure_total_capacity(
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

pub(super) fn allocate_tabs(
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

pub(super) fn active_loads(
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

pub(super) fn unique_accounts_for_allocations(
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

pub(super) fn tab_accounts_for_plan(
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

pub(super) fn total_capacity(accounts: &[BrowserAccount]) -> u16 {
    accounts
        .iter()
        .map(|account| account.max_tabs.max(1))
        .fold(0u16, u16::saturating_add)
}

pub(super) fn release_lease_ids(
    lease_path: &Path,
    lease_ids: &[String],
) -> Result<(), BrowserRegistryError> {
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

pub(super) fn with_locked_leases<T>(
    lease_path: &Path,
    update: impl FnOnce(&mut BrowserLeaseState) -> Result<T, BrowserRegistryError>,
) -> Result<T, BrowserRegistryError> {
    if let Some(parent) = lease_path.parent() {
        ensure_private_dir(parent)?;
    }
    let lock_path = lock_path_for_lease_path(lease_path);
    let lock_file = private_file(&lock_path, false)?;
    let _guard = InterprocessFileLock::lock(lock_file, &lock_path)?;

    let mut state = load_lease_state(lease_path)?;
    let result = update(&mut state)?;
    save_lease_state(lease_path, &state)?;
    Ok(result)
}

pub(super) fn load_lease_state(path: &Path) -> Result<BrowserLeaseState, BrowserRegistryError> {
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

pub(super) fn save_lease_state(
    path: &Path,
    state: &BrowserLeaseState,
) -> Result<(), BrowserRegistryError> {
    let bytes = serde_json::to_vec_pretty(state).map_err(|source| BrowserRegistryError::Write {
        path: path.display().to_string(),
        source: io::Error::new(io::ErrorKind::InvalidData, source),
    })?;
    atomic_write(path, &bytes)
}

pub(super) fn lease_path_for_registry(registry_path: &Path) -> PathBuf {
    let file_name = registry_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("browser-profiles.json");
    registry_path.with_file_name(format!("{file_name}.leases.json"))
}

pub(super) fn lock_path_for_lease_path(lease_path: &Path) -> PathBuf {
    let file_name = lease_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("browser-profiles.json.leases.json");
    lease_path.with_file_name(format!(".{file_name}.lock"))
}

pub(super) fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs()
}

pub(super) fn default_lease_state_version() -> u16 {
    1
}

#[cfg(target_os = "linux")]
pub(super) fn owner_process_is_alive(pid: u32) -> bool {
    pid == std::process::id() || PathBuf::from(format!("/proc/{pid}")).exists()
}

#[cfg(not(target_os = "linux"))]
pub(super) fn owner_process_is_alive(_pid: u32) -> bool {
    true
}

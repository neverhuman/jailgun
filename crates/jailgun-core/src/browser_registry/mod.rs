mod account;
mod ids;
mod leases;
mod registry;
mod storage;

pub use account::{BrowserAccount, BrowserAccountRoots, BrowserAccountStatus};
pub use ids::{default_account_id, validate_account_id};
pub use leases::{
    BrowserLease, BrowserLeaseAllocation, BrowserLeaseManager, BrowserLeaseRequest,
    DEFAULT_BROWSER_QUEUE_TIMEOUT_SECONDS, MAX_BROWSER_QUEUE_TIMEOUT_SECONDS,
};
pub use registry::BrowserProfileRegistry;
pub use storage::{default_registry_path, ensure_private_dir};

use thiserror::Error;

pub const DEFAULT_BROWSER_REGISTRY_ENV: &str = "JAILGUN_BROWSER_PROFILES";
pub const DEFAULT_BROWSER_REGISTRY_RELATIVE_PATH: &str = ".jailgun/browser-profiles.json";

#[derive(Debug, Error)]
pub enum BrowserRegistryError {
    #[error("could not read browser profile registry {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("could not parse browser profile registry {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("could not write browser profile registry {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("could not create private browser runtime directory {path}: {source}")]
    CreatePrivateDir {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("browser account {0} is not registered")]
    MissingAccount(String),
    #[error("browser account {id} is not ready: status={status}")]
    AccountNotReady { id: String, status: String },
    #[error("no ready browser accounts are registered")]
    NoReadyAccounts,
    #[error("duplicate browser account id requested: {0}")]
    DuplicateAccountId(String),
    #[error("requested {requested} browser tab(s), but selected ready accounts allow {capacity}")]
    InsufficientAccountCapacity { requested: u16, capacity: u16 },
    #[error("browser account capacity is busy for {requested} requested tab(s)")]
    LeaseBusy { requested: u16 },
    #[error("browser account capacity is unavailable for {requested} requested tab(s)")]
    LeaseUnavailable { requested: u16 },
    #[error("invalid browser lease request: {0}")]
    LeaseInvalid(String),
    #[error("could not lock browser lease file {path}: {source}")]
    Lock {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("browser account id is required")]
    EmptyAccountId,
    #[error("browser account id is invalid: {0}")]
    InvalidAccountId(String),
}

#[cfg(test)]
mod tests;

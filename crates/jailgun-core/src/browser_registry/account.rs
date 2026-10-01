use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{
    storage::{ensure_private_dir, home_dir_or_current},
    BrowserRegistryError,
};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BrowserAccountStatus {
    #[default]
    AuthRequired,
    Ready,
    Degraded,
    Locked,
    ManualBrowserRequired,
}

impl BrowserAccountStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AuthRequired => "auth-required",
            Self::Ready => "ready",
            Self::Degraded => "degraded",
            Self::Locked => "locked",
            Self::ManualBrowserRequired => "manual-browser-required",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserAccount {
    pub id: String,
    pub email_hint: String,
    pub profile_dir: PathBuf,
    pub state_dir: PathBuf,
    pub downloads_dir: PathBuf,
    pub cdp_port: u16,
    pub max_tabs: u16,
    #[serde(default)]
    pub status: BrowserAccountStatus,
    #[serde(default)]
    pub last_verified_at: Option<String>,
    #[serde(default)]
    pub provider_account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderIdentity {
    pub id: String,
    pub email: String,
}

impl BrowserAccount {
    pub fn require_archive_access(&self) -> Result<(), BrowserRegistryError> {
        let marker = self.profile_dir.join(super::profile_ownership::MARKER);
        match std::fs::symlink_metadata(&marker) {
            Ok(_) => Err(BrowserRegistryError::LeaseInvalid(
                "workflow-runtime-owned: manage this profile through the concept daemon".into(),
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(BrowserRegistryError::Read {
                path: marker.display().to_string(),
                source,
            }),
        }
    }
    pub fn confirm_identity(
        &mut self,
        identity: &ProviderIdentity,
        verified_at: String,
    ) -> Result<(), BrowserRegistryError> {
        if identity.id.trim().is_empty()
            || !self.email_hint.eq_ignore_ascii_case(identity.email.trim())
            || self
                .provider_account_id
                .as_ref()
                .is_some_and(|id| id != &identity.id)
        {
            return Err(BrowserRegistryError::AccountIdentityConflict(
                self.id.clone(),
            ));
        }
        self.provider_account_id = Some(identity.id.clone());
        self.status = BrowserAccountStatus::Ready;
        self.last_verified_at = Some(verified_at);
        Ok(())
    }

    pub fn cdp_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.cdp_port)
    }

    pub fn ensure_runtime_dirs(&self) -> Result<(), BrowserRegistryError> {
        ensure_private_dir(&self.profile_dir)?;
        ensure_private_dir(&self.state_dir)?;
        ensure_private_dir(&self.downloads_dir)?;
        Ok(())
    }

    pub fn require_ready(&self) -> Result<(), BrowserRegistryError> {
        if self.status == BrowserAccountStatus::Ready {
            Ok(())
        } else {
            Err(BrowserRegistryError::AccountNotReady {
                id: self.id.clone(),
                status: self.status.as_str().into(),
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserAccountRoots {
    pub profile_root: PathBuf,
    pub state_root: PathBuf,
    pub downloads_root: PathBuf,
}

impl BrowserAccountRoots {
    pub fn default_under_home() -> Self {
        let root = home_dir_or_current().join(".jailgun");
        Self {
            profile_root: root.join("profiles"),
            state_root: root.join("browser-state"),
            downloads_root: root.join("downloads"),
        }
    }
}

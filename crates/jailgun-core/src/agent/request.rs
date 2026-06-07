use std::{collections::BTreeMap, path::PathBuf};

use crate::{DEFAULT_BROWSER_QUEUE_TIMEOUT_SECONDS, MAX_BROWSER_QUEUE_TIMEOUT_SECONDS};
use serde::{Deserialize, Serialize};

pub const JAILGUN_AGENT_INTERFACE_VERSION: u16 = 1;
pub const JAILGUN_AGENT_MAX_RUNTIME_SECONDS: u64 = 30 * 60;
pub const JAILGUN_AGENT_MAX_TABS: u16 = 5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JailgunAgentRunRequest {
    #[serde(default = "default_interface_version")]
    pub version: u16,
    #[serde(default)]
    pub run_id: Option<String>,
    pub prompt_ref: String,
    pub prompt_file: PathBuf,
    #[serde(default)]
    pub config_path: Option<PathBuf>,
    #[serde(default)]
    pub tabs: Option<u16>,
    #[serde(default)]
    pub max_runtime_seconds: Option<u64>,
    #[serde(default)]
    pub repo: JailgunRepoRef,
    #[serde(default)]
    pub source_archive: JailgunSourceArchiveRequest,
    #[serde(default)]
    pub deploy: JailgunAgentDeployRequest,
    #[serde(default)]
    pub ci: JailgunCiRequest,
    #[serde(default)]
    pub browser: JailgunAgentBrowserRequest,
    #[serde(default)]
    pub github: JailgunGithubPolicyRequest,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct JailgunRepoRef {
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub ref_name: Option<String>,
    #[serde(default)]
    pub base_sha: Option<String>,
    #[serde(default)]
    pub head_sha: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct JailgunSourceArchiveRequest {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub repo_url: Option<String>,
    #[serde(default)]
    pub ref_name: Option<String>,
    #[serde(default)]
    pub tar_target_name: Option<String>,
    #[serde(default)]
    pub expected_top_level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JailgunAgentDeployRequest {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub dry_run: bool,
    #[serde(default)]
    pub allow_live: bool,
    #[serde(default)]
    pub remote_host: Option<String>,
    #[serde(default)]
    pub remote_dir: Option<String>,
    #[serde(default)]
    pub remote_command: Option<String>,
    #[serde(default)]
    pub expected_top_level: Option<String>,
}

impl Default for JailgunAgentDeployRequest {
    fn default() -> Self {
        Self {
            enabled: false,
            dry_run: true,
            allow_live: false,
            remote_host: None,
            remote_dir: None,
            remote_command: None,
            expected_top_level: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct JailgunCiRequest {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub max_attempts: Option<u32>,
    #[serde(default)]
    pub poll_seconds: Option<u16>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct JailgunAgentBrowserRequest {
    #[serde(default)]
    pub profile_dir: Option<PathBuf>,
    #[serde(default)]
    pub profile_pool: Vec<PathBuf>,
    #[serde(default)]
    pub account_ids: Vec<String>,
    #[serde(default)]
    pub allow_queueing: bool,
    #[serde(default)]
    pub queue_timeout_seconds: Option<u64>,
    #[serde(default)]
    pub downloads_dir: Option<PathBuf>,
    #[serde(default)]
    pub artifacts_dir: Option<PathBuf>,
    #[serde(default)]
    pub bridge_cmd: Vec<String>,
    #[serde(default)]
    pub bridge_env: BTreeMap<String, String>,
    #[serde(default)]
    pub event_buffer: Option<usize>,
    #[serde(default)]
    pub deploy_concurrency: Option<u16>,
    #[serde(default)]
    pub download_target_name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct JailgunGithubPolicyRequest {
    #[serde(default)]
    pub allow_write_prompts: bool,
    #[serde(default)]
    pub allow_info_prompts: bool,
    #[serde(default)]
    pub allowed_repositories: Vec<String>,
}

impl JailgunAgentRunRequest {
    pub fn validate_for_config_tabs(&self, config_tabs: u16) -> Result<(), String> {
        if self.version != JAILGUN_AGENT_INTERFACE_VERSION {
            return Err(format!(
                "unsupported Jailgun agent interface version {}; expected {}",
                self.version, JAILGUN_AGENT_INTERFACE_VERSION
            ));
        }
        if self.prompt_ref.trim().is_empty() {
            return Err("prompt_ref is required".into());
        }
        if self.prompt_file.as_os_str().is_empty() {
            return Err("prompt_file is required".into());
        }
        if let Some(run_id) = self.run_id.as_deref() {
            validate_run_id(run_id)?;
        }
        self.effective_tabs(config_tabs)?;
        self.effective_max_runtime_seconds()?;
        self.effective_browser_queue_timeout_seconds()?;
        if self.deploy.enabled && !self.deploy.dry_run && !self.deploy.allow_live {
            return Err("live deploy requires deploy.allow_live=true".into());
        }
        if self.github.allow_write_prompts && self.github.allowed_repositories.is_empty() {
            return Err("github.allow_write_prompts requires allowed_repositories".into());
        }
        Ok(())
    }

    pub fn effective_tabs(&self, config_tabs: u16) -> Result<u16, String> {
        let tabs = self.tabs.unwrap_or(config_tabs);
        if tabs == 0 {
            return Err("tabs must be positive".into());
        }
        if tabs > JAILGUN_AGENT_MAX_TABS {
            return Err(format!(
                "tabs must be <= {}; got {}",
                JAILGUN_AGENT_MAX_TABS, tabs
            ));
        }
        Ok(tabs)
    }

    pub fn effective_max_runtime_seconds(&self) -> Result<u64, String> {
        let seconds = self
            .max_runtime_seconds
            .unwrap_or(JAILGUN_AGENT_MAX_RUNTIME_SECONDS);
        if seconds == 0 {
            return Err("max_runtime_seconds must be positive".into());
        }
        if seconds > JAILGUN_AGENT_MAX_RUNTIME_SECONDS {
            return Err(format!(
                "max_runtime_seconds must be <= {}; got {}",
                JAILGUN_AGENT_MAX_RUNTIME_SECONDS, seconds
            ));
        }
        Ok(seconds)
    }

    pub fn effective_browser_queue_timeout_seconds(&self) -> Result<u64, String> {
        let seconds = self
            .browser
            .queue_timeout_seconds
            .unwrap_or(DEFAULT_BROWSER_QUEUE_TIMEOUT_SECONDS);
        if seconds == 0 {
            return Err("browser.queue_timeout_seconds must be positive".into());
        }
        Ok(seconds.min(MAX_BROWSER_QUEUE_TIMEOUT_SECONDS))
    }
}

pub fn validate_run_id(run_id: &str) -> Result<(), String> {
    let trimmed = run_id.trim();
    if trimmed.is_empty() {
        return Err("run_id cannot be empty".into());
    }
    if trimmed == "." || trimmed == ".." {
        return Err("run_id cannot be a path segment".into());
    }
    if trimmed.len() > 128 {
        return Err("run_id must be 128 characters or fewer".into());
    }
    if !trimmed
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err("run_id may only contain ASCII letters, digits, '.', '_' and '-'".into());
    }
    Ok(())
}

fn default_interface_version() -> u16 {
    JAILGUN_AGENT_INTERFACE_VERSION
}

fn default_true() -> bool {
    true
}

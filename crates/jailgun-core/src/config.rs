use std::{fs, path::Path};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    agent_error::{AgentError, AgentErrorExt},
    browser_registry::DEFAULT_BROWSER_REGISTRY_ENV,
    prompt_policy::PromptPolicy,
    source_archive::SourceArchiveConfig,
};

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not read config {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("could not parse config {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("invalid config: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JailgunConfig {
    pub project: ProjectConfig,
    pub browser: BrowserConfig,
    pub paths: PathConfig,
    pub source_archive: SourceArchiveConfig,
    pub deploy: DeployConfig,
    pub prompt_policy: PromptPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectConfig {
    pub name: String,
    pub repository: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserConfig {
    pub chat_url: String,
    pub model: String,
    pub tabs: u16,
    pub poll_interval_seconds: u16,
    pub completion_check_seconds: u16,
    pub submit_delay_seconds: u16,
    pub submit_jitter_seconds: u16,
    pub tar_wait_minutes: u16,
    pub profile_dir_env: String,
    pub state_dir_env: String,
    #[serde(default = "default_profile_registry_env")]
    pub profile_registry_env: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathConfig {
    pub artifacts_dir: String,
    pub downloads_dir_env: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CleanupPolicy {
    Block,
    PreserveReset,
    Adopt,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeployConfig {
    pub enabled: bool,
    pub dry_run: bool,
    pub remote_host_env: String,
    pub remote_dir_env: String,
    pub remote_command_env: String,
    pub remote_strip_components: u16,
    pub remote_cleanup_policy: CleanupPolicy,
    pub remote_status_poll_seconds: u16,
    pub remote_job_delay_seconds: u16,
    pub remote_job_jitter_seconds: u16,
}

impl JailgunConfig {
    pub fn from_toml_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.display().to_string(),
            source,
        })?;
        let config: Self = toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.display().to_string(),
            source,
        })?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.project.name.trim().is_empty() {
            return Err(ConfigError::Invalid("project.name is required".into()));
        }
        if self.browser.tabs == 0 {
            return Err(ConfigError::Invalid("browser.tabs must be positive".into()));
        }
        if self.browser.poll_interval_seconds == 0 || self.browser.poll_interval_seconds > 10 {
            return Err(ConfigError::Invalid(
                "browser.poll_interval_seconds must be between 1 and 10".into(),
            ));
        }
        if self.browser.completion_check_seconds == 0 {
            return Err(ConfigError::Invalid(
                "browser.completion_check_seconds must be positive".into(),
            ));
        }
        if self.deploy.enabled {
            if self.deploy.remote_host_env.trim().is_empty()
                || self.deploy.remote_dir_env.trim().is_empty()
            {
                return Err(ConfigError::Invalid(
                    "deploy remote host and directory env names are required when deploy is enabled".into(),
                ));
            }
            if self.deploy.remote_status_poll_seconds == 0 {
                return Err(ConfigError::Invalid(
                    "deploy.remote_status_poll_seconds must be positive".into(),
                ));
            }
        }
        self.source_archive
            .validate()
            .map_err(ConfigError::Invalid)?;
        Ok(())
    }

    pub fn redacted_for_display(&self) -> serde_json::Value {
        serde_json::json!({
            "project": {
                "name": self.project.name,
                "repository": self.project.repository,
            },
            "browser": self.browser,
            "paths": self.paths,
            "source_archive": self.source_archive,
            "deploy": {
                "enabled": self.deploy.enabled,
                "dry_run": self.deploy.dry_run,
                "remote_host": { "from_env": self.deploy.remote_host_env },
                "remote_dir": { "from_env": self.deploy.remote_dir_env },
                "remote_command": { "from_env": self.deploy.remote_command_env },
                "remote_strip_components": self.deploy.remote_strip_components,
                "remote_cleanup_policy": self.deploy.remote_cleanup_policy,
                "remote_status_poll_seconds": self.deploy.remote_status_poll_seconds,
                "remote_job_delay_seconds": self.deploy.remote_job_delay_seconds,
                "remote_job_jitter_seconds": self.deploy.remote_job_jitter_seconds,
            },
            "prompt_policy": self.prompt_policy,
        })
    }
}

impl AgentErrorExt for ConfigError {
    fn agent_error(&self) -> AgentError {
        let (code, reason) = match self {
            ConfigError::Read { path, source } => {
                ("config-read", format!("could not read {path}: {source}"))
            }
            ConfigError::Parse { path, source } => {
                ("config-parse", format!("could not parse {path}: {source}"))
            }
            ConfigError::Invalid(message) => ("config-invalid", message.clone()),
        };
        AgentError::new(
            code,
            "load and validate Jailgun configuration",
            reason,
            vec![
                "run config validation against config/jailgun.example.toml",
                "check required environment variable names",
                "keep local overrides in ignored config files",
            ],
            "docs/testing.md",
            "rerun `cargo run -p jailgun-cli -- validate-config --config <path>`",
        )
    }
}

impl Default for JailgunConfig {
    fn default() -> Self {
        Self {
            project: ProjectConfig {
                name: "example-project".into(),
                repository: "git@example.com:org/example-project.git".into(),
            },
            browser: BrowserConfig {
                chat_url: "https://chatgpt.com/".into(),
                model: "pro-extended".into(),
                tabs: 5,
                poll_interval_seconds: 10,
                completion_check_seconds: 2,
                submit_delay_seconds: 60,
                submit_jitter_seconds: 10,
                tar_wait_minutes: 30,
                profile_dir_env: "JAILGUN_CHROME_PROFILE_DIR".into(),
                state_dir_env: "JAILGUN_CHROME_STATE_DIR".into(),
                profile_registry_env: DEFAULT_BROWSER_REGISTRY_ENV.into(),
            },
            paths: PathConfig {
                artifacts_dir: "artifacts".into(),
                downloads_dir_env: "JAILGUN_DOWNLOADS_DIR".into(),
            },
            source_archive: SourceArchiveConfig::default(),
            deploy: DeployConfig {
                enabled: false,
                dry_run: true,
                remote_host_env: "JAILGUN_REMOTE_HOST".into(),
                remote_dir_env: "JAILGUN_REMOTE_DIR".into(),
                remote_command_env: "JAILGUN_REMOTE_COMMAND".into(),
                remote_strip_components: 1,
                remote_cleanup_policy: CleanupPolicy::PreserveReset,
                remote_status_poll_seconds: 30,
                remote_job_delay_seconds: 90,
                remote_job_jitter_seconds: 30,
            },
            prompt_policy: PromptPolicy {
                deny_github_write_by_default: true,
                allow_write_prompts: false,
                allow_info_prompts: false,
                allowed_repositories: Vec::new(),
            },
        }
    }
}

fn default_profile_registry_env() -> String {
    DEFAULT_BROWSER_REGISTRY_ENV.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_example_config_and_redacts_env_values() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../config/jailgun.example.toml"
        );
        let config = JailgunConfig::from_toml_path(path).expect("example config parses");
        assert_eq!(
            config.deploy.remote_cleanup_policy,
            CleanupPolicy::PreserveReset
        );

        let redacted = config.redacted_for_display();
        assert_eq!(
            redacted["deploy"]["remote_host"]["from_env"],
            "JAILGUN_REMOTE_HOST"
        );
        assert!(redacted.to_string().contains("JAILGUN_REMOTE_DIR"));
    }

    #[test]
    fn rejects_slow_tab_polling() {
        let mut config = JailgunConfig::default();
        config.browser.poll_interval_seconds = 11;
        let error = config.validate().expect_err("slow polling is rejected");
        assert!(error.to_string().contains("poll_interval_seconds"));
    }
}

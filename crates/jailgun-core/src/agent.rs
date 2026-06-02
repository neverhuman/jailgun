use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::TarValidation;

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
        self.effective_tabs(config_tabs)?;
        self.effective_max_runtime_seconds()?;
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JailgunAgentRunSummary {
    pub version: u16,
    pub run_id: String,
    pub status: String,
    pub prompt_ref: String,
    pub tab_count: u16,
    pub max_runtime_seconds: u64,
    pub repo_ref: JailgunRepoRef,
    pub source_archive: JailgunSourceArchiveSummary,
    pub deploy_status: String,
    pub ci_status: String,
    pub changed_files: Vec<String>,
    pub artifacts: Vec<JailgunArtifact>,
    pub failures: Vec<JailgunFailure>,
    pub events_jsonl: PathBuf,
    pub receipt_paths: Vec<PathBuf>,
    pub started_at: String,
    pub finished_at: String,
    pub denied_github_prompts: u32,
    pub allowed_info_prompts: u32,
    pub github_write_prompts_allowed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JailgunSourceArchiveSummary {
    pub enabled: bool,
    pub repo_url: String,
    pub ref_name: String,
    pub prefix: String,
    pub archive_filename: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JailgunArtifact {
    pub kind: String,
    pub path: PathBuf,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub size_bytes: Option<u64>,
    #[serde(default)]
    pub receipt_path: Option<PathBuf>,
    #[serde(default)]
    pub tar_validation: Option<TarValidation>,
    #[serde(default)]
    pub changed_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JailgunFailure {
    #[serde(default)]
    pub tab_id: Option<u16>,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JailgunReviewPacket {
    pub version: u16,
    pub generated_at: String,
    pub run_id: String,
    pub prompt_ref: String,
    pub base_sha: String,
    pub head_sha: String,
    pub diff_stat: String,
    pub name_status: Vec<JailgunChangedFile>,
    pub patch: String,
    pub changed_tests: Vec<String>,
    pub summary: JailgunAgentRunSummary,
    pub artifacts: Vec<JailgunArtifact>,
    pub receipt_paths: Vec<PathBuf>,
    pub source_metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JailgunChangedFile {
    pub status: String,
    pub path: String,
    #[serde(default)]
    pub old_path: Option<String>,
}

fn default_interface_version() -> u16 {
    JAILGUN_AGENT_INTERFACE_VERSION
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> JailgunAgentRunRequest {
        JailgunAgentRunRequest {
            version: JAILGUN_AGENT_INTERFACE_VERSION,
            run_id: Some("run-1".into()),
            prompt_ref: "jmcp://work-orders/1/prompt".into(),
            prompt_file: PathBuf::from("/tmp/prompt.txt"),
            config_path: None,
            tabs: Some(2),
            max_runtime_seconds: Some(120),
            repo: JailgunRepoRef::default(),
            source_archive: JailgunSourceArchiveRequest::default(),
            deploy: JailgunAgentDeployRequest::default(),
            ci: JailgunCiRequest::default(),
            browser: JailgunAgentBrowserRequest::default(),
            github: JailgunGithubPolicyRequest::default(),
        }
    }

    #[test]
    fn request_defaults_to_dry_run_deploy_off() {
        let value: JailgunAgentRunRequest = serde_json::from_value(serde_json::json!({
            "prompt_ref": "jmcp://prompt/1",
            "prompt_file": "/tmp/prompt.txt"
        }))
        .expect("request");

        assert!(!value.deploy.enabled);
        assert!(value.deploy.dry_run);
        assert!(!value.deploy.allow_live);
        value.validate_for_config_tabs(1).expect("valid default");
    }

    #[test]
    fn request_rejects_runtime_and_tab_caps() {
        let mut value = request();
        value.tabs = Some(JAILGUN_AGENT_MAX_TABS + 1);
        assert!(value
            .validate_for_config_tabs(1)
            .unwrap_err()
            .contains("tabs"));

        value.tabs = Some(1);
        value.max_runtime_seconds = Some(JAILGUN_AGENT_MAX_RUNTIME_SECONDS + 1);
        assert!(value
            .validate_for_config_tabs(1)
            .unwrap_err()
            .contains("max_runtime_seconds"));
    }

    #[test]
    fn request_rejects_live_deploy_without_explicit_allow() {
        let mut value = request();
        value.deploy.enabled = true;
        value.deploy.dry_run = false;

        assert!(value
            .validate_for_config_tabs(1)
            .unwrap_err()
            .contains("allow_live"));

        value.deploy.allow_live = true;
        value
            .validate_for_config_tabs(1)
            .expect("explicit live allow");
    }

    #[test]
    fn request_requires_repo_scope_for_github_write_allow() {
        let mut value = request();
        value.github.allow_write_prompts = true;

        assert!(value
            .validate_for_config_tabs(1)
            .unwrap_err()
            .contains("allowed_repositories"));

        value.github.allowed_repositories = vec!["org/example".into()];
        value
            .validate_for_config_tabs(1)
            .expect("repo scoped allow");
    }

    #[test]
    fn summary_shape_excludes_prompt_text() {
        let secret_prompt = "implement private customer request";
        let summary = JailgunAgentRunSummary {
            version: JAILGUN_AGENT_INTERFACE_VERSION,
            run_id: "run-1".into(),
            status: "succeeded".into(),
            prompt_ref: "jmcp://prompt/1".into(),
            tab_count: 1,
            max_runtime_seconds: 60,
            repo_ref: JailgunRepoRef::default(),
            source_archive: JailgunSourceArchiveSummary {
                enabled: false,
                repo_url: "git@example.com:org/repo.git".into(),
                ref_name: "HEAD".into(),
                prefix: "source/".into(),
                archive_filename: "source.tar.gz".into(),
            },
            deploy_status: "disabled".into(),
            ci_status: "disabled".into(),
            changed_files: Vec::new(),
            artifacts: Vec::new(),
            failures: vec![JailgunFailure {
                tab_id: None,
                code: "example".into(),
                message: "safe failure".into(),
            }],
            events_jsonl: PathBuf::from("events.jsonl"),
            receipt_paths: Vec::new(),
            started_at: "2026-06-01T00:00:00Z".into(),
            finished_at: "2026-06-01T00:00:01Z".into(),
            denied_github_prompts: 0,
            allowed_info_prompts: 0,
            github_write_prompts_allowed: false,
        };

        let json = serde_json::to_string(&summary).expect("summary json");
        assert!(!json.contains(secret_prompt));
        assert!(json.contains("prompt_ref"));
        assert!(!json.contains("prompt_text"));
    }
}

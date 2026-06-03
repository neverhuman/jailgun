use std::{collections::BTreeMap, path::PathBuf};

use jailgun_core::JailgunConfig;

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub run_id: String,
    pub config: JailgunConfig,
    pub prompt_text: String,
    pub tabs_override: Option<u16>,
    pub initial_tab_burst: Option<u16>,
    pub loop_count: u16,
    pub no_deploy: bool,
    pub dry_run: bool,
    pub profile_dir: PathBuf,
    pub profile_pool: Vec<PathBuf>,
    pub downloads_dir: PathBuf,
    pub artifacts_dir: PathBuf,
    pub bridge_cmd: Vec<String>,
    pub bridge_env: BTreeMap<String, String>,
    pub repo_url: String,
    pub fresh_source_clone: bool,
    pub deploy_remote_host: Option<String>,
    pub deploy_remote_dir: Option<String>,
    pub deploy_remote_command: Option<String>,
    pub deploy_expected_top_level: Option<String>,
    pub ci_tracker_enabled: bool,
    pub ci_repo: Option<String>,
    pub ci_branch: String,
    pub ci_max_attempts: u32,
    pub ci_poll_seconds: u16,
    pub status_max_minutes: u16,
    pub event_buffer: usize,
    pub deploy_concurrency: u16,
}

impl RunOptions {
    pub fn batch_tabs(&self) -> u16 {
        self.tabs_override.unwrap_or(self.config.browser.tabs)
    }

    pub fn tabs(&self) -> u16 {
        self.batch_tabs()
    }

    pub fn planned_tabs(&self) -> Option<u16> {
        let batch_tabs = self.batch_tabs();
        let batch_count = self.loop_count.checked_add(1)?;
        let planned_tabs = batch_tabs.checked_mul(batch_count)?;
        (planned_tabs > 0).then_some(planned_tabs)
    }
}

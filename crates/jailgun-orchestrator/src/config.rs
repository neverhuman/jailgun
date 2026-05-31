use std::{collections::BTreeMap, path::PathBuf};

use jailgun_core::JailgunConfig;

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub run_id: String,
    pub config: JailgunConfig,
    pub prompt_text: String,
    pub tabs_override: Option<u16>,
    pub no_deploy: bool,
    pub dry_run: bool,
    pub profile_dir: PathBuf,
    pub downloads_dir: PathBuf,
    pub artifacts_dir: PathBuf,
    pub bridge_cmd: Vec<String>,
    pub bridge_env: BTreeMap<String, String>,
    pub repo_url: String,
    pub event_buffer: usize,
    pub deploy_concurrency: u16,
}

impl RunOptions {
    pub fn tabs(&self) -> u16 {
        self.tabs_override.unwrap_or(self.config.browser.tabs)
    }
}

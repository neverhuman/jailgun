use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use super::request::JailgunRepoRef;
use crate::TarValidation;

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

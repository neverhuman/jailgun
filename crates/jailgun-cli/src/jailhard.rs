use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fs::{self, File},
    path::{Component, Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};
use clap::Parser;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use jailgun_core::{
    sha256_file, validate_account_id, BrowserAccount, BrowserAccountStatus, BrowserProfileRegistry,
    EventKind, JailgunConfig,
};
use jailgun_orchestrator::{
    run_orchestration,
    support::{bridge_command, default_managed_chrome_profile_dir, default_run_id},
    RunOptions, RunSummary,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tar::{Archive, Builder, EntryType, Header};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::sync::broadcast;

const DEFAULT_MAX_BYTES: u64 = 209_715_200;
const DEFAULT_ROUTER_URL: &str = "http://127.0.0.1:8765/mcp";
const SOURCE_ARCHIVE_FILENAME: &str = "source.tar.gz";

#[derive(Debug, Clone, Parser)]
#[command(name = "jailhard")]
#[command(about = "Run Jailgun hardening over local source and apply the returned patch archive")]
pub struct JailhardArgs {
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,
    #[arg(long = "account")]
    pub accounts: Vec<String>,
    #[arg(long)]
    pub tabs: Option<u16>,
    #[arg(long, default_value = "config/jailgun.example.toml")]
    pub config: PathBuf,
    #[arg(long, default_value = DEFAULT_ROUTER_URL)]
    pub router_url: String,
    #[arg(long)]
    pub download_only: bool,
    #[arg(long)]
    pub no_apply: bool,
    #[arg(long)]
    pub keep_temp: bool,
    #[arg(long = "target-count", value_parser = parse_target_count)]
    pub target_count: Option<u16>,
    #[arg(long = "task-file", value_name = "PATH")]
    pub task_file: Option<PathBuf>,
    /// Restrict the source archive to EXACTLY the files listed in this manifest (newline- or
    /// comma-separated repo-relative paths; blank lines and `#` comments are ignored). Unlike the
    /// default scope walk, this bypasses the source-extension allowlist so curated non-code payload
    /// files (e.g. `.zyal`, `.cff`) are included — while still enforcing the security denylist
    /// (no secrets, no `.git`/`target`/artifact dirs, no path traversal, the `--max-bytes` cap).
    #[arg(long = "include-manifest", value_name = "PATH")]
    pub include_manifest: Option<PathBuf>,
    #[arg(long, default_value_t = DEFAULT_MAX_BYTES)]
    pub max_bytes: u64,
    #[arg(long, hide = true, num_args = 1.., value_name = "ARG", allow_hyphen_values = true)]
    pub bridge_cmd: Vec<String>,
    #[arg(long = "bridge-env", hide = true, value_name = "KEY=VALUE")]
    pub bridge_env: Vec<String>,
}

#[derive(Debug, Clone)]
struct SelectedFile {
    abs_path: PathBuf,
    entry_path: PathBuf,
    size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SourceManifest {
    invocation_dir: String,
    target_paths: Vec<String>,
    selected_files: Vec<ManifestFile>,
    archive_path: String,
    archive_sha256: String,
    archive_size_bytes: u64,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestFile {
    path: String,
    size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JailhardReceipt {
    run_id: String,
    invocation_dir: String,
    target_paths: Vec<String>,
    source_archive: ArchiveReceipt,
    download: Option<DownloadReceipt>,
    apply: ApplyReceipt,
    review: Option<ReviewReceipt>,
    receipt_path: String,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ArchiveReceipt {
    path: String,
    sha256: String,
    size_bytes: u64,
    selected_file_count: usize,
    manifest_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DownloadReceipt {
    path: String,
    sha256: String,
    size_bytes: u64,
    validation: Option<ReturnedArchiveValidation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApplyReceipt {
    applied: bool,
    skipped_reason: Option<String>,
    file_count: usize,
    diff_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReviewReceipt {
    status: String,
    worker_count: u64,
    router_job_id: Option<String>,
    high_risk_supporting_models: u64,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReturnedArchiveValidation {
    files: Vec<String>,
    size_bytes: u64,
}

#[derive(Debug, Clone)]
struct TargetScope {
    roots: Vec<ScopeRoot>,
    all: bool,
}

#[derive(Debug, Clone)]
struct ScopeRoot {
    rel: PathBuf,
    is_file: bool,
}

#[derive(Debug, Clone)]
struct ReviewGateResult {
    status: String,
    worker_count: u64,
    router_job_id: Option<String>,
    high_risk_supporting_models: u64,
}

mod archive;
mod browser;
mod git;
mod prompt;
mod review;
mod scope;
mod source_walk;
mod util;

use archive::*;
use browser::*;
use git::*;
use prompt::*;
use review::*;
use scope::*;
use util::*;

#[cfg(test)]
mod tests;

mod run;

pub use run::run;

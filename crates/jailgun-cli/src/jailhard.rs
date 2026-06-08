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

pub async fn run(args: JailhardArgs) -> Result<()> {
    let invocation_dir = env::current_dir().context("resolving current directory")?;
    let config_path = resolve_config_path(&args.config);
    let mut config = JailgunConfig::from_toml_path(&config_path)
        .with_context(|| format!("loading {}", config_path.display()))?;
    config.source_archive.enabled = true;
    config.source_archive.archive_filename = SOURCE_ARCHIVE_FILENAME.into();
    config.source_archive.delete_after_upload = true;
    config.deploy.enabled = false;
    config.deploy.dry_run = true;
    config.deploy.remote_strip_components = 0;
    config
        .validate()
        .context("validating jailhard-adjusted config")?;

    let apply_requested = !args.download_only && !args.no_apply;
    if apply_requested {
        ensure_clean_worktree(&invocation_dir)?;
    }

    // When --include-manifest is set, the archive is exactly the listed files (extension allowlist
    // bypassed, security denylist still enforced); otherwise fall back to the default scope walk.
    let manifest_paths = match args.include_manifest.as_deref() {
        Some(path) => Some(read_manifest_paths(&invocation_dir, path)?),
        None => None,
    };
    let scope = match &manifest_paths {
        Some(paths) => TargetScope::resolve(&invocation_dir, paths)?,
        None => TargetScope::resolve(&invocation_dir, &args.paths)?,
    };
    let temp_dir = tempfile::Builder::new()
        .prefix("jailgun-hardening-")
        .tempdir_in("/tmp")
        .context("creating /tmp jailhard archive root")?;
    let temp_dir_path = temp_dir.path().to_path_buf();
    let source_archive = source_archive_path(&temp_dir_path)?;
    let selected = match &manifest_paths {
        Some(_) => select_manifest_source_files(&invocation_dir, &scope)?,
        None => select_source_files(&invocation_dir, &scope)?,
    };
    let manifest = create_source_archive(
        &invocation_dir,
        &scope,
        &selected,
        &source_archive,
        args.max_bytes,
    )?;

    let run_id = default_run_id();
    let artifacts_dir = absolute_from(&invocation_dir, Path::new(&config.paths.artifacts_dir));
    let jailhard_dir = artifacts_dir.join("jailhard").join(&run_id);
    fs::create_dir_all(&jailhard_dir)
        .with_context(|| format!("creating {}", jailhard_dir.display()))?;
    let manifest_path = jailhard_dir.join("source-manifest.json");
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)
        .with_context(|| format!("writing {}", manifest_path.display()))?;

    let task_override = match args.task_file.as_deref() {
        Some(path) => Some(read_task_file(path)?),
        None => None,
    };
    let prompt_path = prompt_work_path(&temp_dir_path)?;
    fs::write(
        &prompt_path,
        hardening_prompt(&manifest, args.target_count, task_override.as_deref()),
    )
    .with_context(|| format!("writing {}", prompt_path.display()))?;

    let accounts = resolve_accounts(&config, &args.accounts)?;
    let default_tabs = u16::try_from(accounts.len())
        .context("browser account count exceeds supported tab count")?;
    let tabs = args.tabs.unwrap_or(default_tabs).max(1);
    ensure_account_capacity(&accounts, tabs)?;

    let downloads_dir = artifacts_dir.join("downloads");
    let mut bridge_env = parse_env_overrides(args.bridge_env)?;
    bridge_env.insert(
        "JAILGUN_DOWNLOADS_DIR".into(),
        downloads_dir.display().to_string(),
    );
    bridge_env.insert(
        "JAILGUN_ARTIFACTS_DIR".into(),
        artifacts_dir.display().to_string(),
    );
    bridge_env.insert("JAILGUN_TAR_TARGET_NAME".into(), "source.tar.gz".into());
    apply_account_profile_env(&mut bridge_env, &config, &accounts)?;

    let bridge_cmd = bridge_command(args.bridge_cmd)?;
    let opts = RunOptions {
        run_id: run_id.clone(),
        config: config.clone(),
        prompt_text: fs::read_to_string(&prompt_path)?,
        tabs_override: Some(tabs),
        no_deploy: true,
        dry_run: true,
        profile_dir: accounts
            .first()
            .map(|account| account.profile_dir.clone())
            .unwrap_or_else(default_managed_chrome_profile_dir),
        profile_pool: accounts
            .iter()
            .map(|account| account.profile_dir.clone())
            .collect(),
        tab_profile_dirs: Default::default(),
        downloads_dir,
        artifacts_dir: artifacts_dir.clone(),
        bridge_cmd,
        bridge_env,
        repo_url: "local://jailhard".into(),
        local_archive_path: Some(source_archive.clone()),
        deploy_remote_host: None,
        deploy_remote_dir: None,
        deploy_remote_command: None,
        deploy_expected_top_level: None,
        ci_tracker_enabled: false,
        ci_repo: None,
        ci_branch: "main".into(),
        ci_max_attempts: 1,
        ci_poll_seconds: 30,
        status_max_minutes: config.browser.tar_wait_minutes,
        max_runtime_seconds: None,
        event_buffer: 1024,
        deploy_concurrency: 1,
    };

    let run_result = run_browser_workflow(opts).await?;
    if !run_result.summary.failures.is_empty() {
        anyhow::bail!(
            "jailhard run {} finished with failures: {:?}",
            run_id,
            run_result.summary.failures
        );
    }
    let downloaded = run_result
        .downloaded
        .last()
        .cloned()
        .context("jailhard run finished without a downloaded .tar.gz")?;
    let downloaded_sha = sha256_file(&downloaded)
        .with_context(|| format!("hashing downloaded archive {}", downloaded.display()))?;
    let downloaded_size = fs::metadata(&downloaded)
        .with_context(|| format!("stat {}", downloaded.display()))?
        .len();

    let mut validation = None;
    let mut apply_receipt = ApplyReceipt {
        applied: false,
        skipped_reason: None,
        file_count: 0,
        diff_sha256: None,
    };
    let mut review_receipt = None;
    let mut fatal_after_receipt = None;

    if args.download_only {
        apply_receipt.skipped_reason = Some("download-only".into());
    } else {
        let returned = validate_returned_archive(&downloaded, &invocation_dir, &scope, &manifest)?;
        apply_receipt.file_count = returned.files.len();
        validation = Some(returned.clone());
        if args.no_apply {
            apply_receipt.skipped_reason = Some("no-apply".into());
        } else {
            ensure_clean_worktree(&invocation_dir)?;
            unpack_validated_archive(&downloaded, &invocation_dir, &returned.files)?;
            apply_receipt.applied = true;
            let diff = git_diff_binary(&invocation_dir)?;
            let diff_sha = sha256_bytes(diff.as_bytes());
            apply_receipt.diff_sha256 = Some(diff_sha);
            apply_receipt.skipped_reason = None;
            match review_patch(&args.router_url, &diff).await {
                Ok(review) => {
                    if review.high_risk_supporting_models >= 2 {
                        fatal_after_receipt = Some(format!(
                            "router review gate rejected patch: {} supporting model(s) reported high-risk correctness/regression/security findings",
                            review.high_risk_supporting_models
                        ));
                    }
                    review_receipt = Some(ReviewReceipt {
                        status: review.status,
                        worker_count: review.worker_count,
                        router_job_id: review.router_job_id,
                        high_risk_supporting_models: review.high_risk_supporting_models,
                        error: fatal_after_receipt.clone(),
                    });
                }
                Err(error) => {
                    let message = error.to_string();
                    fatal_after_receipt = Some(message.clone());
                    review_receipt = Some(ReviewReceipt {
                        status: "failed".into(),
                        worker_count: 0,
                        router_job_id: None,
                        high_risk_supporting_models: 0,
                        error: Some(message),
                    });
                }
            }
        }
    }

    let receipt_path = jailhard_dir.join("receipt.json");
    let receipt = JailhardReceipt {
        run_id: run_id.clone(),
        invocation_dir: invocation_dir.display().to_string(),
        target_paths: scope.display_roots(),
        source_archive: ArchiveReceipt {
            path: source_archive.display().to_string(),
            sha256: manifest.archive_sha256.clone(),
            size_bytes: manifest.archive_size_bytes,
            selected_file_count: manifest.selected_files.len(),
            manifest_path: manifest_path.display().to_string(),
        },
        download: Some(DownloadReceipt {
            path: downloaded.display().to_string(),
            sha256: downloaded_sha,
            size_bytes: downloaded_size,
            validation,
        }),
        apply: apply_receipt,
        review: review_receipt,
        receipt_path: receipt_path.display().to_string(),
        created_at: timestamp_now(),
    };
    fs::write(&receipt_path, serde_json::to_vec_pretty(&receipt)?)
        .with_context(|| format!("writing {}", receipt_path.display()))?;

    if args.keep_temp {
        eprintln!("kept jailhard working dir: {}", temp_dir_path.display());
        std::mem::forget(temp_dir);
    }

    println!("{}", serde_json::to_string_pretty(&receipt)?);
    if let Some(message) = fatal_after_receipt {
        anyhow::bail!(message);
    }
    Ok(())
}

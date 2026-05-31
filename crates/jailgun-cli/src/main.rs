use std::{env, net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::{Parser, Subcommand, ValueEnum};
use jailgun_core::{repo_policy, validate_tar_gz, CleanupPolicy, JailgunConfig, TarValidation};
use jailgun_deploy::{
    cleanup_remote_checkout, deploy_remote,
    shell::{SshRemoteGit, SshRemoteJob, SshRemoteUpload},
    CiState, CiTracker, CleanupRequest, DeployError, DeployReceipt, DeployRequest,
    JsonReceiptWriter,
};
use jailgun_notify::{
    build_commit_message, collect_commit_summary, read_chat_id_cache, send_telegram_message,
    write_chat_id_cache, CommitSummary, TelegramConfig,
};
use jailgun_server::{api_router, router_with_static, AppState};

#[derive(Debug, Parser)]
#[command(name = "jailgun")]
#[command(about = "Rust core for ChatGPT archive capture and safe deploy")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    ValidateConfig {
        #[arg(long, default_value = "config/jailgun.example.toml")]
        config: PathBuf,
    },
    TarValidate {
        archive: PathBuf,
        #[arg(long)]
        require_single_top_level: bool,
    },
    Scan {
        paths: Vec<PathBuf>,
    },
    RemoteCleanup {
        #[arg(long, default_value = "config/jailgun.example.toml")]
        config: PathBuf,
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        tab_id: Option<u16>,
        #[arg(long)]
        remote_host: Option<String>,
        #[arg(long)]
        remote_dir: Option<String>,
        #[arg(long)]
        receipt_dir: Option<PathBuf>,
        #[arg(long, value_enum)]
        policy: Option<CleanupPolicyArg>,
    },
    DeployArchive {
        archive: PathBuf,
        #[arg(long, default_value = "config/jailgun.example.toml")]
        config: PathBuf,
        #[arg(long)]
        run_id: String,
        #[arg(long, default_value_t = 1)]
        tab_id: u16,
        #[arg(long)]
        remote_host: Option<String>,
        #[arg(long)]
        remote_dir: Option<String>,
        #[arg(long)]
        remote_command: Option<String>,
        #[arg(long)]
        receipt_dir: Option<PathBuf>,
        #[arg(long, value_enum)]
        policy: Option<CleanupPolicyArg>,
        #[arg(long)]
        dry_run: bool,
        /// Refuse deploy when the archive's single top-level directory is not this value.
        #[arg(long, default_value = "jekko")]
        expected_top_level: String,
        #[arg(long, default_value_t = 360)]
        status_max_minutes: u16,
        #[arg(long)]
        ci: bool,
    },
    TelegramSend {
        #[arg(long, default_value = "telegram/token.env")]
        token_file: PathBuf,
        #[arg(long, default_value = "telegram/chat_id.cache")]
        chat_id_cache: PathBuf,
        #[arg(long)]
        chat_id: Option<String>,
        #[arg(long)]
        message: String,
    },
    NotifyCommit {
        #[arg(long, default_value = "telegram/token.env")]
        token_file: PathBuf,
        #[arg(long, default_value = "telegram/chat_id.cache")]
        chat_id_cache: PathBuf,
        #[arg(long)]
        chat_id: Option<String>,
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long, default_value = "HEAD")]
        revision: String,
    },
    Serve {
        #[arg(long, default_value = "config/jailgun.example.toml")]
        config: PathBuf,
        #[arg(long, default_value = "127.0.0.1:8787")]
        addr: SocketAddr,
        #[arg(long)]
        dashboard_dist: Option<PathBuf>,
        /// Start with a live broadcast bus (AppState::live). The /ws/events
        /// endpoint streams events forwarded via POST /api/events instead of
        /// replaying the fixture once.
        #[arg(long)]
        live: bool,
        /// Required for POST /api/events. When unset, the endpoint returns 503.
        #[arg(long, env = "JAILGUN_INGEST_TOKEN")]
        ingest_token: Option<String>,
    },
    Fixture {
        #[arg(value_enum)]
        kind: FixtureKind,
    },
}

#[derive(Debug, Clone, ValueEnum)]
enum FixtureKind {
    Runs,
    Config,
}

#[derive(Debug, Clone, ValueEnum)]
enum CleanupPolicyArg {
    Block,
    PreserveReset,
    Adopt,
}

impl From<CleanupPolicyArg> for CleanupPolicy {
    fn from(value: CleanupPolicyArg) -> Self {
        match value {
            CleanupPolicyArg::Block => CleanupPolicy::Block,
            CleanupPolicyArg::PreserveReset => CleanupPolicy::PreserveReset,
            CleanupPolicyArg::Adopt => CleanupPolicy::Adopt,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    match cli.command {
        Command::ValidateConfig { config } => {
            let config = JailgunConfig::from_toml_path(&config)
                .with_context(|| format!("validating {}", config.display()))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&config.redacted_for_display())?
            );
        }
        Command::TarValidate {
            archive,
            require_single_top_level,
        } => {
            let validation = validate_tar_gz(&archive, require_single_top_level)
                .with_context(|| format!("validating {}", archive.display()))?;
            println!("{}", serde_json::to_string_pretty(&validation)?);
        }
        Command::Scan { paths } => {
            let mut findings = Vec::new();
            for path in paths {
                if path.is_file() {
                    findings.extend(repo_policy::scan_file(&path)?);
                }
            }
            if !findings.is_empty() {
                println!("{}", serde_json::to_string_pretty(&findings)?);
                anyhow::bail!("personal string scan found {} issue(s)", findings.len());
            }
            println!("[]");
        }
        Command::RemoteCleanup {
            config,
            run_id,
            tab_id,
            remote_host,
            remote_dir,
            receipt_dir,
            policy,
        } => {
            let config = JailgunConfig::from_toml_path(&config)
                .with_context(|| format!("loading {}", config.display()))?;
            let remote_host =
                arg_or_env(remote_host, &config.deploy.remote_host_env, "remote host")?;
            let remote_dir = arg_or_env(remote_dir, &config.deploy.remote_dir_env, "remote dir")?;
            let receipt_dir = receipt_dir
                .unwrap_or_else(|| PathBuf::from(&config.paths.artifacts_dir).join("receipts"));
            let policy = policy
                .map(CleanupPolicy::from)
                .unwrap_or(config.deploy.remote_cleanup_policy);
            let mut backend = SshRemoteGit::new(remote_host.clone(), receipt_dir.clone());
            let receipt = cleanup_remote_checkout(
                &mut backend,
                CleanupRequest {
                    run_id,
                    tab_id,
                    remote_host,
                    remote_dir,
                    policy,
                    receipt_dir,
                },
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&receipt)?);
        }
        Command::DeployArchive {
            archive,
            config,
            run_id,
            tab_id,
            remote_host,
            remote_dir,
            remote_command,
            receipt_dir,
            policy,
            dry_run,
            expected_top_level,
            status_max_minutes,
            ci,
        } => {
            let config = JailgunConfig::from_toml_path(&config)
                .with_context(|| format!("loading {}", config.display()))?;
            let remote_host =
                arg_or_env(remote_host, &config.deploy.remote_host_env, "remote host")?;
            let remote_dir = arg_or_env(remote_dir, &config.deploy.remote_dir_env, "remote dir")?;
            let remote_command = match remote_command {
                Some(value) => value,
                None => env::var(&config.deploy.remote_command_env).unwrap_or_else(|_| {
                    if config.deploy.remote_command_env == "JAILGUN_REMOTE_COMMAND" {
                        "bash ci-fast-push.sh".into()
                    } else {
                        String::new()
                    }
                }),
            };
            let receipt_dir = receipt_dir
                .unwrap_or_else(|| PathBuf::from(&config.paths.artifacts_dir).join("receipts"));
            let policy = policy
                .map(CleanupPolicy::from)
                .unwrap_or(config.deploy.remote_cleanup_policy);
            let validation = validate_tar_gz(&archive, config.deploy.remote_strip_components > 0)
                .with_context(|| format!("validating {}", archive.display()))?;
            ensure_expected_top_level(&validation, &expected_top_level)?;
            eprintln!(
                "validated archive: {} bytes, {} entries, top_level={}",
                validation.size_bytes,
                validation.entry_count,
                validation.top_level.as_deref().unwrap_or("(multiple)")
            );

            let mut git = SshRemoteGit::new(remote_host.clone(), receipt_dir.clone());
            let cleanup = cleanup_remote_checkout(
                &mut git,
                CleanupRequest {
                    run_id: run_id.clone(),
                    tab_id: Some(tab_id),
                    remote_host: remote_host.clone(),
                    remote_dir: remote_dir.clone(),
                    policy,
                    receipt_dir: receipt_dir.clone(),
                },
            )
            .await?;
            eprintln!(
                "remote cleanup outcome: {:?} preserved_ref={}",
                cleanup.outcome,
                cleanup.preserved_ref.as_deref().unwrap_or("-")
            );

            let archive_name = archive
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("chatgpt-output.tar.gz")
                .to_string();
            let (events, _rx) = tokio::sync::broadcast::channel(128);
            let mut upload = SshRemoteUpload::new(remote_host.clone());
            let mut job = SshRemoteJob::new(remote_host.clone());
            let mut ci_tracker = NoCiTracker;
            let mut writer = LocalReceiptWriter {
                receipt_dir: receipt_dir.clone(),
            };
            let receipt = deploy_remote(
                &mut upload,
                &mut job,
                &mut ci_tracker,
                &mut writer,
                DeployRequest {
                    run_id,
                    tab_id,
                    remote_host,
                    remote_dir,
                    remote_command,
                    remote_archive_basename: archive_name,
                    local_archive_path: archive,
                    strip_components: config.deploy.remote_strip_components,
                    cleanup_policy: policy,
                    receipt_dir,
                    status_poll_seconds: config.deploy.remote_status_poll_seconds,
                    status_max_minutes,
                    ci_tracker_enabled: ci,
                    ci_branch: "main".into(),
                    ci_max_attempts: 20,
                    ci_poll_seconds: 30,
                    stash_on_failure: true,
                    dry_run,
                },
                &events,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&receipt)?);
        }
        Command::TelegramSend {
            token_file,
            chat_id_cache,
            chat_id,
            message,
        } => {
            let mut config = TelegramConfig::from_token_file(&token_file)
                .with_context(|| format!("loading {}", token_file.display()))?;
            if let Some(chat_id) = chat_id {
                config.chat_id = Some(chat_id);
            }
            if config.chat_id.is_none() {
                config.chat_id = read_chat_id_cache(&chat_id_cache)?;
            }
            let sent_chat_id = send_telegram_message(&config, &message).await?;
            write_chat_id_cache(&chat_id_cache, &sent_chat_id)?;
            println!(
                "{}",
                serde_json::json!({
                    "status": "sent",
                    "chat_id": sent_chat_id,
                })
            );
        }
        Command::NotifyCommit {
            token_file,
            chat_id_cache,
            chat_id,
            repo,
            revision,
        } => {
            let mut config = TelegramConfig::from_token_file(&token_file)
                .with_context(|| format!("loading {}", token_file.display()))?;
            if let Some(chat_id) = chat_id {
                config.chat_id = Some(chat_id);
            }
            if config.chat_id.is_none() {
                config.chat_id = read_chat_id_cache(&chat_id_cache)?;
            }
            let summary = collect_commit_summary(&repo, &revision)
                .with_context(|| format!("collecting commit summary for {revision}"))?;
            let message = build_commit_message(&summary);
            let sent_chat_id = send_telegram_message(&config, &message).await?;
            write_chat_id_cache(&chat_id_cache, &sent_chat_id)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&notify_result(sent_chat_id, summary))?
            );
        }
        Command::Serve {
            config,
            addr,
            dashboard_dist,
            live,
            ingest_token,
        } => {
            let config = JailgunConfig::from_toml_path(&config)
                .with_context(|| format!("loading {}", config.display()))?;
            let receipt_dir = PathBuf::from(&config.paths.artifacts_dir).join("receipts");
            let state = if live {
                let (state, _rx) = AppState::live(config, receipt_dir, 1024);
                state.with_ingest_token(ingest_token)
            } else {
                AppState::fixture(config)
            };
            let router = match dashboard_dist {
                Some(dir) => router_with_static(state, dir),
                None => api_router(state),
            };
            println!("listening on http://{addr} (live={live})");
            jailgun_server::serve(addr, router).await?;
        }
        Command::Fixture { kind } => {
            let config = JailgunConfig::default();
            let state = AppState::fixture(config);
            match kind {
                FixtureKind::Runs => println!("{}", serde_json::to_string_pretty(&state.runs)?),
                FixtureKind::Config => println!(
                    "{}",
                    serde_json::to_string_pretty(&state.config.redacted_for_display())?
                ),
            }
        }
    }
    Ok(())
}

fn notify_result(chat_id: String, summary: CommitSummary) -> serde_json::Value {
    serde_json::json!({
        "status": "sent",
        "chat_id": chat_id,
        "commit": summary.short_hash,
        "subject": summary.subject,
        "files": summary.files,
    })
}

fn arg_or_env(value: Option<String>, env_name: &str, label: &str) -> Result<String> {
    match value {
        Some(value) if !value.trim().is_empty() => Ok(value),
        _ => env::var(env_name)
            .with_context(|| format!("{label} must be provided or set in ${env_name}"))
            .and_then(|value| {
                if value.trim().is_empty() {
                    anyhow::bail!("{label} from ${env_name} is empty");
                }
                Ok(value)
            }),
    }
}

fn ensure_expected_top_level(validation: &TarValidation, expected: &str) -> Result<()> {
    let expected = expected.trim();
    if !expected.is_empty() && validation.top_level.as_deref() != Some(expected) {
        anyhow::bail!(
            "archive top-level must be {expected}/, found {}; refusing remote upload",
            validation.top_level.as_deref().unwrap_or("(multiple)")
        );
    }
    Ok(())
}

struct LocalReceiptWriter {
    receipt_dir: PathBuf,
}

#[async_trait]
impl JsonReceiptWriter for LocalReceiptWriter {
    async fn write_receipt(&mut self, receipt: &DeployReceipt) -> Result<PathBuf, DeployError> {
        tokio::fs::create_dir_all(&self.receipt_dir).await?;
        let path = self.receipt_dir.join(format!(
            "{}-tab-{:02}-deploy.json",
            receipt.run_id, receipt.tab_id
        ));
        let bytes = serde_json::to_vec_pretty(receipt)?;
        tokio::fs::write(&path, bytes).await?;
        Ok(path)
    }
}

struct NoCiTracker;

#[async_trait]
impl CiTracker for NoCiTracker {
    async fn check(&mut self, _commit_sha: &str, _branch: &str) -> Result<CiState, DeployError> {
        Ok(CiState::Skipped {
            reason: "cli-disabled".into(),
        })
    }

    async fn capture_failure_log(
        &mut self,
        _run_id: &str,
        _max_bytes: usize,
    ) -> Result<String, DeployError> {
        Ok(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validation_with_top_level(top_level: Option<&str>) -> TarValidation {
        TarValidation {
            size_bytes: 1,
            entry_count: 1,
            files: Vec::new(),
            top_levels: top_level
                .map(|value| vec![value.to_string()])
                .unwrap_or_else(|| vec!["jekko".to_string(), "other".to_string()]),
            top_level: top_level.map(str::to_string),
        }
    }

    #[test]
    fn deploy_archive_accepts_expected_top_level() {
        let validation = validation_with_top_level(Some("jekko"));
        ensure_expected_top_level(&validation, "jekko").unwrap();
    }

    #[test]
    fn deploy_archive_rejects_jekko_fixes_top_level() {
        let validation = validation_with_top_level(Some("jekko-fixes"));
        let error = ensure_expected_top_level(&validation, "jekko").unwrap_err();
        assert!(error
            .to_string()
            .contains("archive top-level must be jekko/, found jekko-fixes"));
    }

    #[test]
    fn deploy_archive_rejects_multiple_top_levels() {
        let validation = validation_with_top_level(None);
        let error = ensure_expected_top_level(&validation, "jekko").unwrap_err();
        assert!(error
            .to_string()
            .contains("archive top-level must be jekko/, found (multiple)"));
    }
}

use std::{
    collections::BTreeMap,
    env, fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::{Parser, Subcommand, ValueEnum};
use jailgun_core::{
    repo_policy, validate_tar_gz, CleanupPolicy, JailgunConfig, JailgunEvent, TarValidation,
};
use jailgun_deploy::{
    cleanup_remote_checkout, deploy_remote,
    shell::{SshCiTracker, SshRemoteGit, SshRemoteJob, SshRemoteUpload},
    CleanupRequest, DeployError, DeployOutcome, DeployReceipt, DeployRequest, JsonReceiptWriter,
};
use jailgun_notify::{
    build_commit_message, collect_commit_summary, commit_notice_to_payload, run_jmcp_subscriber,
    BatchRequestPayload, CommitNotice, CommitSummary, JmcpEnvelope, JmcpInbox, NotifyTextPayload,
    Payload, Routing, TaskRef,
};
use jailgun_server::{api_router, router_with_static, AppState};
use tokio::sync::broadcast;

#[derive(Debug, Parser)]
#[command(name = "jailgun")]
#[command(about = "Rust core for ChatGPT archive capture and safe deploy")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
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
        #[arg(long)]
        ci_repo: Option<String>,
    },
    Run {
        #[arg(long, default_value = "config/jailgun.example.toml")]
        config: PathBuf,
        #[arg(long)]
        prompt_file: PathBuf,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long)]
        tabs: Option<u16>,
        #[arg(long, default_value_t = 0, env = "JAILGUN_LOOPS")]
        loops: u16,
        #[arg(long)]
        source_repo_url: Option<String>,
        #[arg(long)]
        source_ref: Option<String>,
        #[arg(long)]
        submit_delay_seconds: Option<u16>,
        #[arg(long)]
        submit_jitter_seconds: Option<u16>,
        #[arg(long)]
        submit_jitter_percent: Option<u16>,
        #[arg(long)]
        fresh_source_clone: bool,
        #[arg(long)]
        deploy: bool,
        #[arg(long)]
        no_deploy: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        remote_host: Option<String>,
        #[arg(long)]
        remote_dir: Option<String>,
        #[arg(long)]
        remote_command: Option<String>,
        #[arg(long)]
        expected_top_level: Option<String>,
        #[arg(long)]
        tar_target_name: Option<String>,
        #[arg(long)]
        profile_dir: Option<PathBuf>,
        #[arg(long = "profile-pool", value_name = "DIR")]
        profile_pool: Vec<PathBuf>,
        #[arg(long)]
        downloads_dir: Option<PathBuf>,
        #[arg(long)]
        artifacts_dir: Option<PathBuf>,
        #[arg(long, num_args = 1.., value_name = "ARG", allow_hyphen_values = true)]
        bridge_cmd: Vec<String>,
        #[arg(long = "bridge-env", value_name = "KEY=VALUE")]
        bridge_env: Vec<String>,
        #[arg(long, default_value_t = 1024)]
        event_buffer: usize,
        #[arg(long, default_value_t = 1)]
        deploy_concurrency: u16,
        #[arg(long, default_value_t = 360)]
        status_max_minutes: u16,
        #[arg(long)]
        ci: bool,
        #[arg(long)]
        ci_repo: Option<String>,
        #[arg(long, default_value = "main")]
        ci_branch: String,
        #[arg(long, default_value_t = 20)]
        ci_max_attempts: u32,
        #[arg(long, default_value_t = 30)]
        ci_poll_seconds: u16,
        /// Enable the JMCP notification subscriber. Writes envelopes to
        /// `--jmcp-inbox-dir`. The bridge there picks them up and ships them
        /// to the user; jailgun never touches the Telegram bot directly.
        #[arg(long)]
        notify_jmcp: bool,
        #[arg(long, default_value = "~/code/jmcp/inbox")]
        jmcp_inbox_dir: PathBuf,
        /// Start the live dashboard/API server for this run and stream run
        /// events to its WebSocket replay buffer.
        #[arg(long)]
        serve: bool,
        #[arg(long, default_value = "127.0.0.1:8787")]
        addr: SocketAddr,
        #[arg(long)]
        dashboard_dist: Option<PathBuf>,
        /// Keep the live dashboard/API server alive after the run completes so
        /// external monitors can capture final proof.
        #[arg(long, default_value_t = 0)]
        dashboard_hold_seconds: u64,
        /// Keep the live dashboard/API server attached until Ctrl-C after a
        /// successful run so operators can inspect the final state.
        #[arg(long)]
        dashboard_keep_alive: bool,
    },
    /// Write a one-off JMCP envelope (plain text body) to the outbox.
    JmcpSend {
        #[arg(long, default_value = "~/code/jmcp/inbox")]
        jmcp_inbox_dir: PathBuf,
        #[arg(long)]
        message: String,
        #[arg(long, default_value = "Jailgun notice")]
        title: String,
        #[arg(long, default_value = "📨")]
        summary_emoji: String,
    },
    Runs {
        #[arg(long, default_value = "config/jailgun.example.toml")]
        config: PathBuf,
        #[arg(long)]
        prompt_file: PathBuf,
        #[arg(long)]
        count: u16,
        #[arg(long = "profile-pool", value_name = "DIR")]
        profile_pool: Vec<PathBuf>,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long, default_value = "~/code/jmcp/inbox")]
        jmcp_inbox_dir: PathBuf,
    },
    /// Write a commit-notice JMCP envelope to the outbox. Invoked by the
    /// post-commit hook so a successful commit is reported via JMCP.
    NotifyCommit {
        #[arg(long, default_value = "~/code/jmcp/inbox")]
        jmcp_inbox_dir: PathBuf,
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
        /// Spawn a JMCP subscriber on the live broadcast that writes
        /// envelopes for three milestones: job started on a tab, tar
        /// acquired, and deploy success with CI passed (or any failure).
        #[arg(long)]
        notify_jmcp: bool,
        #[arg(long, default_value = "~/code/jmcp/inbox")]
        jmcp_inbox_dir: PathBuf,
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
            ci_repo,
        } => {
            let config = JailgunConfig::from_toml_path(&config)
                .with_context(|| format!("loading {}", config.display()))?;
            let ci_repo = ci_repo.or_else(|| infer_github_repo(&config.project.repository));
            let remote_host =
                arg_or_env(remote_host, &config.deploy.remote_host_env, "remote host")?;
            let remote_dir = arg_or_env(remote_dir, &config.deploy.remote_dir_env, "remote dir")?;
            let remote_command =
                deploy_remote_command(remote_command, &config.deploy.remote_command_env)?;
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
            let mut ci_tracker = SshCiTracker::with_repo(ci_repo.clone());
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
                    ci_repo,
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
            if !deploy_outcome_succeeded(receipt.outcome) {
                let mut reason = format!(
                    "deploy finished with outcome {}",
                    deploy_outcome_label(receipt.outcome)
                );
                if let Some(failure_reason) = receipt.final_status.failure_reason.as_deref() {
                    reason.push_str(&format!("; failure_reason={failure_reason}"));
                }
                if let Some(exit_code) = receipt.final_status.exit_code {
                    reason.push_str(&format!("; exit_code={exit_code}"));
                }
                if let Some(line) = receipt
                    .log_tail
                    .lines()
                    .find(|line| !line.trim().is_empty())
                {
                    reason.push_str(&format!("; log_tail={}", line.trim()));
                }
                anyhow::bail!(reason);
            }
        }
        Command::Run {
            config,
            prompt_file,
            run_id,
            tabs,
            loops,
            source_repo_url,
            source_ref,
            submit_delay_seconds,
            submit_jitter_seconds,
            submit_jitter_percent,
            fresh_source_clone,
            deploy,
            no_deploy,
            dry_run,
            remote_host,
            remote_dir,
            remote_command,
            expected_top_level,
            tar_target_name,
            profile_dir,
            profile_pool,
            downloads_dir,
            artifacts_dir,
            bridge_cmd,
            bridge_env,
            event_buffer,
            deploy_concurrency,
            status_max_minutes,
            ci,
            ci_repo,
            ci_branch,
            ci_max_attempts,
            ci_poll_seconds,
            notify_jmcp,
            jmcp_inbox_dir,
            serve,
            addr,
            dashboard_dist,
            dashboard_hold_seconds,
            dashboard_keep_alive,
        } => {
            let mut config = JailgunConfig::from_toml_path(&config)
                .with_context(|| format!("loading {}", config.display()))?;
            let jmcp_inbox_dir = expand_tilde(&jmcp_inbox_dir)?;
            if notify_jmcp {
                validate_jmcp_inbox(&jmcp_inbox_dir)?;
            }
            if let Some(source_ref) = source_ref {
                config.source_archive.ref_name = source_ref;
            }
            if let Some(seconds) = submit_delay_seconds {
                config.browser.submit_delay_seconds = seconds;
            }
            if let Some(seconds) = submit_jitter_seconds {
                config.browser.submit_jitter_seconds = seconds;
            }
            if let Some(percent) = submit_jitter_percent {
                config.browser.submit_jitter_percent = Some(percent);
            }
            config.deploy.enabled = !no_deploy && (deploy || config.deploy.enabled);
            config.deploy.dry_run = resolve_deploy_dry_run(config.deploy.dry_run, deploy, dry_run);

            let prompt_text = fs::read_to_string(&prompt_file)
                .with_context(|| format!("reading prompt file {}", prompt_file.display()))?;
            let artifacts_dir =
                artifacts_dir.unwrap_or_else(|| PathBuf::from(&config.paths.artifacts_dir));
            let downloads_dir = path_arg_or_env_or_default(
                downloads_dir,
                &config.paths.downloads_dir_env,
                artifacts_dir.join("downloads"),
            )?;
            let receipt_dir = artifacts_dir.join("receipts");
            let profile_pool = profile_pool_arg_or_env(profile_pool)?;
            let profile_dir = path_arg_or_env_or_default(
                profile_dir,
                &config.browser.profile_dir_env,
                profile_pool
                    .first()
                    .cloned()
                    .unwrap_or_else(default_managed_chrome_profile_dir),
            )?;
            let repo_url = source_repo_url
                .or_else(|| env::var(&config.source_archive.repo_url_env).ok())
                .unwrap_or_else(|| config.project.repository.clone());
            let ci_repo = ci_repo.or_else(|| infer_github_repo(&repo_url));
            let deploy_remote_host = if config.deploy.enabled {
                Some(arg_or_env(
                    remote_host,
                    &config.deploy.remote_host_env,
                    "remote host",
                )?)
            } else {
                None
            };
            let deploy_remote_dir = if config.deploy.enabled {
                Some(arg_or_env(
                    remote_dir,
                    &config.deploy.remote_dir_env,
                    "remote dir",
                )?)
            } else {
                None
            };
            let deploy_remote_command = if config.deploy.enabled {
                Some(deploy_remote_command(
                    remote_command,
                    &config.deploy.remote_command_env,
                )?)
            } else {
                None
            };
            let mut bridge_env = parse_env_overrides(bridge_env)?;
            bridge_env.insert(
                "JAILGUN_DOWNLOADS_DIR".into(),
                downloads_dir.display().to_string(),
            );
            bridge_env.insert(
                "JAILGUN_ARTIFACTS_DIR".into(),
                artifacts_dir.display().to_string(),
            );
            if let Some(tar_target_name) = tar_target_name {
                bridge_env.insert("JAILGUN_TAR_TARGET_NAME".into(), tar_target_name);
            }
            bridge_env
                .entry(config.browser.profile_dir_env.clone())
                .or_insert_with(|| profile_dir.display().to_string());
            bridge_env
                .entry(config.browser.state_dir_env.clone())
                .or_insert_with(|| default_managed_chrome_state_dir().display().to_string());
            if !profile_pool.is_empty() {
                bridge_env
                    .entry("JAILGUN_CHROME_PROFILE_POOL".into())
                    .or_insert(profile_pool_env_value(&profile_pool)?);
            }
            let bridge_cmd = bridge_command(bridge_cmd)?;
            let run_id = run_id.unwrap_or_else(default_run_id);
            let live_dashboard = if serve {
                Some(
                    start_live_dashboard(
                        config.clone(),
                        receipt_dir,
                        event_buffer,
                        addr,
                        dashboard_dist,
                    )
                    .await?,
                )
            } else {
                None
            };
            let opts = jailgun_orchestrator::RunOptions {
                run_id,
                config,
                prompt_text,
                tabs_override: tabs,
                loop_count: loops,
                no_deploy,
                dry_run,
                profile_dir,
                profile_pool,
                downloads_dir,
                artifacts_dir,
                bridge_cmd,
                bridge_env,
                repo_url,
                fresh_source_clone,
                deploy_remote_host,
                deploy_remote_dir,
                deploy_remote_command,
                deploy_expected_top_level: expected_top_level,
                ci_tracker_enabled: ci,
                ci_repo,
                ci_branch,
                ci_max_attempts,
                ci_poll_seconds,
                status_max_minutes,
                event_buffer,
                deploy_concurrency,
            };
            let handle = jailgun_orchestrator::run_orchestration(opts).await?;
            let dashboard_forwarder = live_dashboard.as_ref().map(|dashboard| {
                spawn_run_event_forwarder(handle.events_rx.resubscribe(), dashboard.state.clone())
            });
            if notify_jmcp {
                tokio::spawn(run_jmcp_subscriber(
                    handle.events_rx.resubscribe(),
                    jmcp_inbox_dir.clone(),
                ));
            }
            let result = stream_run(handle).await;
            if let Some(task) = dashboard_forwarder {
                task.abort();
            }
            let summary = match result {
                Ok(summary) => summary,
                Err(error) => {
                    if let Some(dashboard) = live_dashboard.as_ref() {
                        dashboard.server_task.abort();
                    }
                    return Err(error);
                }
            };
            if !summary.failures.is_empty() {
                if let Some(dashboard) = live_dashboard.as_ref() {
                    dashboard.server_task.abort();
                }
                anyhow::bail!(
                    "run completed with {} failure(s): {:?}",
                    summary.failures.len(),
                    summary.failures
                );
            }
            if serve && dashboard_keep_alive {
                eprintln!("dashboard keep-alive enabled; press Ctrl-C to stop");
                let _ = tokio::signal::ctrl_c().await;
            } else if serve && dashboard_hold_seconds > 0 {
                eprintln!("dashboard hold for {dashboard_hold_seconds}s before shutdown");
                tokio::time::sleep(Duration::from_secs(dashboard_hold_seconds)).await;
            }
            if let Some(dashboard) = live_dashboard {
                dashboard.server_task.abort();
            }
        }
        Command::JmcpSend {
            jmcp_inbox_dir,
            message,
            title,
            summary_emoji,
        } => {
            let jmcp_inbox_dir = expand_tilde(&jmcp_inbox_dir)?;
            let inbox = JmcpInbox::new(&jmcp_inbox_dir);
            let payload = Payload::NotifyText(NotifyTextPayload {
                title,
                summary_emoji,
                body_markdown: message,
            });
            let envelope = JmcpEnvelope::new(
                payload,
                TaskRef::for_run("jailgun-cli", None),
                Routing::notify_user(),
            );
            let path = inbox.write_envelope(&envelope).await.with_context(|| {
                format!("writing JMCP envelope to {}", jmcp_inbox_dir.display())
            })?;
            println!(
                "{}",
                serde_json::json!({
                    "status": "queued",
                    "envelope_id": envelope.envelope_id,
                    "path": path,
                })
            );
        }
        Command::Runs {
            config,
            prompt_file,
            count,
            profile_pool,
            run_id,
            jmcp_inbox_dir,
        } => {
            if count == 0 {
                anyhow::bail!("--count must be greater than zero");
            }
            let jmcp_inbox_dir = expand_tilde(&jmcp_inbox_dir)?;
            let inbox = JmcpInbox::new(&jmcp_inbox_dir);
            let batch_id = run_id.unwrap_or_else(default_batch_id);
            let mut child_command = format!(
                "jailgun run --config {} --prompt-file {}",
                shell_quote(&config.display().to_string()),
                shell_quote(&prompt_file.display().to_string())
            );
            for profile_dir in &profile_pool {
                child_command.push_str(" --profile-pool ");
                child_command.push_str(&shell_quote(&profile_dir.display().to_string()));
            }
            let body_markdown = format!(
                "Request approval for {count} jailgun runs.\n\n- config: `{}`\n- prompt file: `{}`\n- profile pool: `{}`\n- child command: `{child_command}`\n- approval required: yes",
                config.display(),
                prompt_file.display(),
                profile_pool_display(&profile_pool)
            );
            let payload = Payload::BatchRequest(BatchRequestPayload {
                title: "Jailgun batch launch requested".to_string(),
                summary_emoji: "🧭".to_string(),
                body_markdown,
                count,
                config_path: config.display().to_string(),
                prompt_file: prompt_file.display().to_string(),
                child_command: child_command.clone(),
                execution_mode: "serial".to_string(),
                approval_required: true,
            });
            let envelope = JmcpEnvelope::new(
                payload,
                TaskRef::for_run(&batch_id, None),
                Routing::notify_user(),
            );
            let path = inbox.write_envelope(&envelope).await.with_context(|| {
                format!(
                    "writing JMCP batch envelope to {}",
                    jmcp_inbox_dir.display()
                )
            })?;
            println!(
                "{}",
                serde_json::json!({
                    "status": "queued",
                    "batch_id": batch_id,
                    "count": count,
                    "approval_required": true,
                    "execution_mode": "serial",
                    "envelope_id": envelope.envelope_id,
                    "envelope_path": path,
                    "child_command": child_command,
                    "config_path": config.display().to_string(),
                    "prompt_file": prompt_file.display().to_string(),
                    "profile_pool": profile_pool.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
                })
            );
        }
        Command::NotifyCommit {
            jmcp_inbox_dir,
            repo,
            revision,
        } => {
            let jmcp_inbox_dir = expand_tilde(&jmcp_inbox_dir)?;
            let inbox = JmcpInbox::new(&jmcp_inbox_dir);
            let summary = collect_commit_summary(&repo, &revision)
                .with_context(|| format!("collecting commit summary for {revision}"))?;
            let notice = CommitNotice {
                run_id: "post-commit-hook".to_string(),
                tab_id: None,
                post_head: summary.short_hash.clone(),
                pre_head: None,
                files_changed: summary.files.len(),
                additions: 0,
                deletions: 0,
                top_paths: summary.files.clone(),
                ci_state: None,
                remote_command_exit: None,
            };
            let payload = commit_notice_to_payload(&notice);
            let envelope = JmcpEnvelope::new(
                payload,
                TaskRef::for_run("post-commit-hook", None),
                Routing::notify_user(),
            );
            let path = inbox.write_envelope(&envelope).await.with_context(|| {
                format!("writing JMCP envelope to {}", jmcp_inbox_dir.display())
            })?;
            // Body markdown stays available for callers that want to render
            // locally (e.g. for shell output debugging).
            let body = build_commit_message(&summary);
            println!(
                "{}",
                serde_json::to_string_pretty(&notify_result(
                    envelope.envelope_id.clone(),
                    path,
                    summary,
                    body
                ))?
            );
        }
        Command::Serve {
            config,
            addr,
            dashboard_dist,
            live,
            ingest_token,
            notify_jmcp,
            jmcp_inbox_dir,
        } => {
            let config = JailgunConfig::from_toml_path(&config)
                .with_context(|| format!("loading {}", config.display()))?;
            let receipt_dir = PathBuf::from(&config.paths.artifacts_dir).join("receipts");
            let jmcp_inbox_dir = expand_tilde(&jmcp_inbox_dir)?;
            let state = if live {
                let (state, rx) = AppState::live(config, receipt_dir, 1024);
                if notify_jmcp {
                    validate_jmcp_inbox(&jmcp_inbox_dir)?;
                    tokio::spawn(run_jmcp_subscriber(rx, jmcp_inbox_dir.clone()));
                }
                state.with_ingest_token(ingest_token)
            } else {
                if notify_jmcp {
                    anyhow::bail!("--notify-jmcp requires --live");
                }
                AppState::fixture(config)
            };
            let router = match dashboard_dist {
                Some(dir) => router_with_static(state, dir),
                None => api_router(state),
            };
            println!("listening on http://{addr} (live={live} notify_jmcp={notify_jmcp})");
            jailgun_server::serve(addr, router).await?;
        }
        Command::Fixture { kind } => {
            let config = JailgunConfig::default();
            let state = AppState::fixture(config);
            match kind {
                FixtureKind::Runs => {
                    let runs = state.runs.read().await;
                    println!("{}", serde_json::to_string_pretty(&*runs)?)
                }
                FixtureKind::Config => println!(
                    "{}",
                    serde_json::to_string_pretty(&state.config.redacted_for_display())?
                ),
            }
        }
    }
    Ok(())
}

fn notify_result(
    envelope_id: String,
    envelope_path: PathBuf,
    summary: CommitSummary,
    body: String,
) -> serde_json::Value {
    serde_json::json!({
        "status": "queued",
        "envelope_id": envelope_id,
        "envelope_path": envelope_path,
        "commit": summary.short_hash,
        "subject": summary.subject,
        "files": summary.files,
        "body": body,
    })
}

fn validate_jmcp_inbox(inbox_dir: &Path) -> Result<()> {
    if let Some(parent) = inbox_dir.parent() {
        if !parent.exists() {
            anyhow::bail!(
                "--notify-jmcp inbox parent {} does not exist; create it before invoking jailgun",
                parent.display()
            );
        }
    }
    std::fs::create_dir_all(inbox_dir)
        .with_context(|| format!("creating JMCP inbox directory {}", inbox_dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(inbox_dir)
            .with_context(|| format!("inspecting JMCP inbox directory {}", inbox_dir.display()))?;
        let mode = metadata.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            anyhow::bail!(
                "JMCP inbox {} is mode {:o}; refusing to use a world-readable directory \
                 (chmod 700)",
                inbox_dir.display(),
                mode
            );
        }
    }
    Ok(())
}

fn expand_tilde(path: &Path) -> Result<PathBuf> {
    let str_path = match path.to_str() {
        Some(value) => value,
        None => return Ok(path.to_path_buf()),
    };
    if let Some(rest) = str_path.strip_prefix("~/") {
        let home = std::env::var_os("HOME").ok_or_else(|| {
            anyhow::anyhow!("cannot expand ~ in {}: HOME is unset", path.display())
        })?;
        let mut expanded = PathBuf::from(home);
        expanded.push(rest);
        Ok(expanded)
    } else if str_path == "~" {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("cannot expand ~: HOME is unset"))
    } else {
        Ok(path.to_path_buf())
    }
}

struct LiveDashboard {
    server_task: tokio::task::JoinHandle<std::io::Result<()>>,
    state: AppState,
}

async fn start_live_dashboard(
    config: JailgunConfig,
    receipt_dir: PathBuf,
    event_buffer: usize,
    addr: SocketAddr,
    dashboard_dist: Option<PathBuf>,
) -> Result<LiveDashboard> {
    let dashboard_dist = resolve_dashboard_dist(dashboard_dist)?;
    let (state, _rx) = AppState::live(config, receipt_dir, event_buffer);
    let router = router_with_static(state.clone(), dashboard_dist.clone());
    let (bound_addr, server_task) = jailgun_server::spawn_server(addr, router)
        .await
        .with_context(|| format!("binding live dashboard server at http://{addr}"))?;
    eprintln!(
        "dashboard listening on http://{bound_addr} (live=true, dist={})",
        dashboard_dist.display()
    );
    Ok(LiveDashboard { server_task, state })
}

fn resolve_dashboard_dist(value: Option<PathBuf>) -> Result<PathBuf> {
    let dir = value.unwrap_or_else(|| PathBuf::from("apps/dashboard/dist"));
    if !dir.join("index.html").is_file() {
        anyhow::bail!(
            "dashboard assets not found at {}; run `npm run build --workspace apps/dashboard` or pass --dashboard-dist",
            dir.display()
        );
    }
    Ok(dir)
}

fn spawn_run_event_forwarder(
    mut rx: broadcast::Receiver<JailgunEvent>,
    state: AppState,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    state.record_event(event).await;
                }
                Err(broadcast::error::RecvError::Lagged(dropped)) => {
                    eprintln!("dashboard event forwarder lagged; dropped {dropped} event(s)");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}

fn infer_github_repo(url: &str) -> Option<String> {
    let value = url.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(rest) = value.strip_prefix("git@github.com:") {
        return owner_repo_from_github_path(rest);
    }
    if let Some((_, rest)) = value.split_once("github.com/") {
        return owner_repo_from_github_path(rest);
    }
    None
}

fn owner_repo_from_github_path(path: &str) -> Option<String> {
    let mut parts = path
        .trim_start_matches('/')
        .split(['/', '#', '?'])
        .filter(|part| !part.is_empty());
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim().trim_end_matches(".git");
    if owner.is_empty() || repo.is_empty() {
        None
    } else {
        Some(format!("{owner}/{repo}"))
    }
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

fn deploy_remote_command(value: Option<String>, env_name: &str) -> Result<String> {
    if let Some(value) = value {
        return Ok(value);
    }
    match env::var(env_name) {
        Ok(value) => Ok(value),
        Err(env::VarError::NotPresent) if env_name == "JAILGUN_REMOTE_COMMAND" => {
            Ok("bash ci-fast-push.sh".to_string())
        }
        Err(env::VarError::NotPresent) => Ok(String::new()),
        Err(env::VarError::NotUnicode(_)) => {
            anyhow::bail!("remote command environment variable ${env_name} is not valid UTF-8")
        }
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

fn resolve_deploy_dry_run(config_dry_run: bool, deploy: bool, dry_run: bool) -> bool {
    if dry_run {
        true
    } else if deploy {
        false
    } else {
        config_dry_run
    }
}

fn deploy_outcome_succeeded(outcome: DeployOutcome) -> bool {
    matches!(
        outcome,
        DeployOutcome::Succeeded | DeployOutcome::SucceededCiSkipped | DeployOutcome::DryRunStaged
    )
}

fn deploy_outcome_label(outcome: DeployOutcome) -> &'static str {
    match outcome {
        DeployOutcome::Succeeded => "succeeded",
        DeployOutcome::SucceededCiFailed => "succeeded-ci-failed",
        DeployOutcome::SucceededCiSkipped => "succeeded-ci-skipped",
        DeployOutcome::FailedPreserved => "failed-preserved",
        DeployOutcome::FailedHard => "failed-hard",
        DeployOutcome::UploadShaMismatch => "upload-sha-mismatch",
        DeployOutcome::TimedOut => "timed-out",
        DeployOutcome::DryRunStaged => "dry-run-staged",
    }
}

fn bridge_command(args: Vec<String>) -> Result<Vec<String>> {
    if !args.is_empty() {
        return Ok(args);
    }
    match env::var("JAILGUN_BRIDGE_CMD") {
        Ok(value) => {
            let parts = value
                .split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>();
            if parts.is_empty() {
                anyhow::bail!("JAILGUN_BRIDGE_CMD is empty");
            }
            Ok(parts)
        }
        Err(env::VarError::NotPresent) => default_bridge_command(),
        Err(env::VarError::NotUnicode(_)) => {
            anyhow::bail!("JAILGUN_BRIDGE_CMD is not valid UTF-8")
        }
    }
}

fn default_bridge_command() -> Result<Vec<String>> {
    for path in default_bridge_candidates() {
        if path.is_file() {
            return Ok(vec!["node".to_string(), path.display().to_string()]);
        }
    }
    anyhow::bail!(
        "bridge command must be provided with --bridge-cmd or JAILGUN_BRIDGE_CMD; default bridge was not found at apps/chrome-bridge/bin/chrome-bridge.mjs"
    )
}

fn default_bridge_candidates() -> Vec<PathBuf> {
    vec![
        PathBuf::from("apps/chrome-bridge/bin/chrome-bridge.mjs"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/chrome-bridge/bin/chrome-bridge.mjs"),
    ]
}

fn parse_env_overrides(values: Vec<String>) -> Result<BTreeMap<String, String>> {
    let mut envs = BTreeMap::new();
    for value in values {
        let Some((key, val)) = value.split_once('=') else {
            anyhow::bail!("--bridge-env must be KEY=VALUE, got {value:?}");
        };
        if key.trim().is_empty() {
            anyhow::bail!("--bridge-env key cannot be empty");
        }
        envs.insert(key.to_string(), val.to_string());
    }
    Ok(envs)
}

fn path_arg_or_env_or_default(
    value: Option<PathBuf>,
    env_name: &str,
    default: PathBuf,
) -> Result<PathBuf> {
    if let Some(value) = value {
        return Ok(value);
    }
    match env::var(env_name) {
        Ok(value) if !value.trim().is_empty() => Ok(PathBuf::from(value)),
        Ok(_) => anyhow::bail!("path environment variable ${env_name} is empty"),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(env::VarError::NotUnicode(_)) => {
            anyhow::bail!("path environment variable ${env_name} is not valid UTF-8")
        }
    }
}

fn profile_pool_arg_or_env(values: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
    if !values.is_empty() {
        return Ok(values);
    }
    for env_name in ["JAILGUN_CHROME_PROFILE_POOL", "JAILGUN_CHROME_PROFILE_DIRS"] {
        match env::var_os(env_name) {
            Some(value) if value.is_empty() => anyhow::bail!("path list ${env_name} is empty"),
            Some(value) => return Ok(env::split_paths(&value).collect()),
            None => {}
        }
    }
    Ok(Vec::new())
}

fn profile_pool_env_value(paths: &[PathBuf]) -> Result<String> {
    let joined = env::join_paths(paths).with_context(|| "joining browser profile pool paths")?;
    joined
        .into_string()
        .map_err(|_| anyhow::anyhow!("browser profile pool contains non-UTF-8 paths"))
}

fn default_managed_chrome_profile_dir() -> PathBuf {
    home_dir_or_current().join(".google-profile-automation-profile")
}

fn default_managed_chrome_state_dir() -> PathBuf {
    home_dir_or_current().join(".google-profile-automation-state")
}

fn home_dir_or_current() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn default_run_id() -> String {
    format!("run-{}", uuid::Uuid::new_v4())
}

fn default_batch_id() -> String {
    format!("batch-{}", uuid::Uuid::new_v4())
}

fn shell_quote(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_string();
    }
    if arg
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | '~' | ':'))
    {
        return arg.to_string();
    }
    format!("'{}'", arg.replace('\'', "'\"'\"'"))
}

fn profile_pool_display(paths: &[PathBuf]) -> String {
    if paths.is_empty() {
        "default single profile".to_string()
    } else {
        paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

async fn stream_run(
    mut handle: jailgun_orchestrator::OrchestratorHandle,
) -> Result<jailgun_orchestrator::RunSummary> {
    let mut events_open = true;
    let summary;
    loop {
        tokio::select! {
            event = handle.events_rx.recv(), if events_open => {
                match event {
                    Ok(event) => println!("{}", serde_json::to_string(&event)?),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(dropped)) => {
                        eprintln!("event stream lagged; dropped {dropped} event(s)");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        events_open = false;
                    }
                }
            }
            completion = &mut handle.completion => {
                let completed = completion.context("orchestrator task ended before sending a summary")?;
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "type": "run-summary",
                        "summary": completed,
                    }))?
                );
                summary = completed;
                break;
            }
        }
    }
    let _ = handle.shutdown.send(true);
    Ok(summary)
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

    #[test]
    fn run_deploy_flag_disables_config_dry_run() {
        assert!(!resolve_deploy_dry_run(true, true, false));
    }

    #[test]
    fn run_dry_run_flag_overrides_deploy() {
        assert!(resolve_deploy_dry_run(false, true, true));
    }

    #[test]
    fn run_without_deploy_preserves_config_dry_run() {
        assert!(resolve_deploy_dry_run(true, false, false));
        assert!(!resolve_deploy_dry_run(false, false, false));
    }

    #[test]
    fn failed_preserved_deploy_archive_outcome_is_not_successful() {
        assert!(deploy_outcome_succeeded(DeployOutcome::Succeeded));
        assert!(deploy_outcome_succeeded(DeployOutcome::SucceededCiSkipped));
        assert!(!deploy_outcome_succeeded(DeployOutcome::FailedPreserved));
        assert!(!deploy_outcome_succeeded(DeployOutcome::SucceededCiFailed));
    }

    #[test]
    fn run_cli_parses_submit_delay_jitter_percent_keep_alive_and_fresh_clone() {
        let cli = Cli::try_parse_from([
            "jailgun",
            "run",
            "--prompt-file",
            "prompt.md",
            "--submit-delay-seconds",
            "120",
            "--submit-jitter-seconds",
            "0",
            "--submit-jitter-percent",
            "20",
            "--dashboard-keep-alive",
            "--fresh-source-clone",
        ])
        .expect("run args parse");
        match cli.command {
            Command::Run {
                submit_delay_seconds,
                submit_jitter_seconds,
                submit_jitter_percent,
                dashboard_keep_alive,
                fresh_source_clone,
                ..
            } => {
                assert_eq!(submit_delay_seconds, Some(120));
                assert_eq!(submit_jitter_seconds, Some(0));
                assert_eq!(submit_jitter_percent, Some(20));
                assert!(dashboard_keep_alive);
                assert!(fresh_source_clone);
            }
            other => panic!("expected run command, got {other:?}"),
        }
    }

    #[test]
    fn run_cli_parses_repeated_profile_pool() {
        let cli = Cli::try_parse_from([
            "jailgun",
            "run",
            "--prompt-file",
            "prompt.md",
            "--profile-pool",
            "/tmp/google-a",
            "--profile-pool",
            "/tmp/google-b",
        ])
        .expect("run args parse");
        match cli.command {
            Command::Run { profile_pool, .. } => {
                assert_eq!(
                    profile_pool,
                    vec![
                        PathBuf::from("/tmp/google-a"),
                        PathBuf::from("/tmp/google-b")
                    ]
                );
            }
            other => panic!("expected run command, got {other:?}"),
        }
    }

    #[test]
    fn runs_cli_parses_repeated_profile_pool() {
        let cli = Cli::try_parse_from([
            "jailgun",
            "runs",
            "--count",
            "2",
            "--prompt-file",
            "prompt.md",
            "--profile-pool",
            "/tmp/google-a",
            "--profile-pool",
            "/tmp/google-b",
        ])
        .expect("runs args parse");
        match cli.command {
            Command::Runs {
                count,
                profile_pool,
                ..
            } => {
                assert_eq!(count, 2);
                assert_eq!(
                    profile_pool,
                    vec![
                        PathBuf::from("/tmp/google-a"),
                        PathBuf::from("/tmp/google-b")
                    ]
                );
            }
            other => panic!("expected runs command, got {other:?}"),
        }
    }

    #[test]
    fn infers_github_owner_repo_from_supported_remote_urls() {
        assert_eq!(
            infer_github_repo("git@github.com:example/repo.git").as_deref(),
            Some("example/repo")
        );
        assert_eq!(
            infer_github_repo("https://github.com/example/repo.git").as_deref(),
            Some("example/repo")
        );
        assert_eq!(
            infer_github_repo("ssh://git@github.com/example/repo.git").as_deref(),
            Some("example/repo")
        );
        assert_eq!(infer_github_repo("git@example.com:org/repo.git"), None);
    }
}

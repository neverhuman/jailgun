use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use jailgun_core::{repo_policy, validate_tar_gz, CleanupPolicy, JailgunConfig};
use jailgun_deploy::{
    cleanup_remote_checkout, deploy_remote,
    shell::{SshCiTracker, SshRemoteGit, SshRemoteJob, SshRemoteUpload},
    CleanupRequest, DeployRequest,
};
use jailgun_notify::{
    build_commit_message, collect_commit_summary, read_chat_id_cache, send_telegram_message,
    write_chat_id_cache, CommitSummary, TelegramConfig,
};
use jailgun_server::{api_router, router_with_static, AppState};

use crate::{
    agent,
    cli::{CleanupPolicyArg, Command, FixtureKind},
};
use jailgun_orchestrator::{
    run_orchestration,
    support::{
        arg_or_env, bridge_command, default_managed_chrome_profile_dir,
        default_managed_chrome_state_dir, default_run_id, deploy_outcome_label,
        deploy_outcome_succeeded, deploy_remote_command, ensure_expected_top_level,
        ensure_parent_dir, infer_github_repo, path_arg_or_env_or_default, LocalReceiptWriter,
    },
};

pub async fn dispatch(command: Command) -> Result<()> {
    match command {
        Command::ValidateConfig { config } => validate_config(config).await,
        Command::TarValidate {
            archive,
            require_single_top_level,
        } => tar_validate(archive, require_single_top_level).await,
        Command::Scan { paths } => scan(paths).await,
        Command::RemoteCleanup {
            config,
            run_id,
            tab_id,
            remote_host,
            remote_dir,
            receipt_dir,
            policy,
        } => {
            remote_cleanup(
                config,
                run_id,
                tab_id,
                remote_host,
                remote_dir,
                receipt_dir,
                policy,
            )
            .await
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
            deploy_archive(
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
            )
            .await
        }
        Command::Run {
            config,
            prompt_file,
            run_id,
            tabs,
            source_repo_url,
            source_ref,
            deploy,
            no_deploy,
            dry_run,
            remote_host,
            remote_dir,
            remote_command,
            expected_top_level,
            tar_target_name,
            profile_dir,
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
            notify_telegram,
            telegram_token_file,
            telegram_chat_id_cache,
        } => {
            run(
                config,
                prompt_file,
                run_id,
                tabs,
                source_repo_url,
                source_ref,
                deploy,
                no_deploy,
                dry_run,
                remote_host,
                remote_dir,
                remote_command,
                expected_top_level,
                tar_target_name,
                profile_dir,
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
                notify_telegram,
                telegram_token_file,
                telegram_chat_id_cache,
            )
            .await
        }
        Command::RunAgent {
            request,
            events_jsonl,
            summary_json,
        } => agent::run_agent(request, events_jsonl, summary_json).await,
        Command::ReviewPacket {
            summary_json,
            base,
            head,
            repo,
            output,
            patch_bytes,
        } => review_packet(summary_json, base, head, repo, output, patch_bytes).await,
        Command::TelegramSend {
            token_file,
            chat_id_cache,
            chat_id,
            message,
        } => telegram_send(token_file, chat_id_cache, chat_id, message).await,
        Command::NotifyCommit {
            token_file,
            chat_id_cache,
            chat_id,
            repo,
            revision,
        } => notify_commit(token_file, chat_id_cache, chat_id, repo, revision).await,
        Command::Serve {
            config,
            addr,
            dashboard_dist,
            live,
            ingest_token,
            notify_telegram,
            telegram_token_file,
            telegram_chat_id_cache,
        } => {
            serve(
                config,
                addr,
                dashboard_dist,
                live,
                ingest_token,
                notify_telegram,
                telegram_token_file,
                telegram_chat_id_cache,
            )
            .await
        }
        Command::Fixture { kind } => fixture(kind).await,
    }
}

async fn validate_config(config: PathBuf) -> Result<()> {
    let config = JailgunConfig::from_toml_path(&config)
        .with_context(|| format!("validating {}", config.display()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&config.redacted_for_display())?
    );
    Ok(())
}

async fn tar_validate(archive: PathBuf, require_single_top_level: bool) -> Result<()> {
    let validation = validate_tar_gz(&archive, require_single_top_level)
        .with_context(|| format!("validating {}", archive.display()))?;
    println!("{}", serde_json::to_string_pretty(&validation)?);
    Ok(())
}

async fn scan(paths: Vec<PathBuf>) -> Result<()> {
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
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn remote_cleanup(
    config: PathBuf,
    run_id: String,
    tab_id: Option<u16>,
    remote_host: Option<String>,
    remote_dir: Option<String>,
    receipt_dir: Option<PathBuf>,
    policy: Option<CleanupPolicyArg>,
) -> Result<()> {
    let config = JailgunConfig::from_toml_path(&config)
        .with_context(|| format!("loading {}", config.display()))?;
    let remote_host = arg_or_env(remote_host, &config.deploy.remote_host_env, "remote host")?;
    let remote_dir = arg_or_env(remote_dir, &config.deploy.remote_dir_env, "remote dir")?;
    let receipt_dir =
        receipt_dir.unwrap_or_else(|| PathBuf::from(&config.paths.artifacts_dir).join("receipts"));
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
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn deploy_archive(
    archive: PathBuf,
    config: PathBuf,
    run_id: String,
    tab_id: u16,
    remote_host: Option<String>,
    remote_dir: Option<String>,
    remote_command: Option<String>,
    receipt_dir: Option<PathBuf>,
    policy: Option<CleanupPolicyArg>,
    dry_run: bool,
    expected_top_level: String,
    status_max_minutes: u16,
    ci: bool,
    ci_repo: Option<String>,
) -> Result<()> {
    let config = JailgunConfig::from_toml_path(&config)
        .with_context(|| format!("loading {}", config.display()))?;
    let ci_repo = ci_repo.or_else(|| infer_github_repo(&config.project.repository));
    let remote_host = arg_or_env(remote_host, &config.deploy.remote_host_env, "remote host")?;
    let remote_dir = arg_or_env(remote_dir, &config.deploy.remote_dir_env, "remote dir")?;
    let remote_command = deploy_remote_command(remote_command, &config.deploy.remote_command_env)?;
    let receipt_dir =
        receipt_dir.unwrap_or_else(|| PathBuf::from(&config.paths.artifacts_dir).join("receipts"));
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
    let mut writer = LocalReceiptWriter::new(receipt_dir.clone());
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
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run(
    config: PathBuf,
    prompt_file: PathBuf,
    run_id: Option<String>,
    tabs: Option<u16>,
    source_repo_url: Option<String>,
    source_ref: Option<String>,
    deploy: bool,
    no_deploy: bool,
    dry_run: bool,
    remote_host: Option<String>,
    remote_dir: Option<String>,
    remote_command: Option<String>,
    expected_top_level: Option<String>,
    tar_target_name: Option<String>,
    profile_dir: Option<PathBuf>,
    downloads_dir: Option<PathBuf>,
    artifacts_dir: Option<PathBuf>,
    bridge_cmd: Vec<String>,
    bridge_env: Vec<String>,
    event_buffer: usize,
    deploy_concurrency: u16,
    status_max_minutes: u16,
    ci: bool,
    ci_repo: Option<String>,
    ci_branch: String,
    ci_max_attempts: u32,
    ci_poll_seconds: u16,
    notify_telegram: bool,
    telegram_token_file: PathBuf,
    telegram_chat_id_cache: PathBuf,
) -> Result<()> {
    let mut config = JailgunConfig::from_toml_path(&config)
        .with_context(|| format!("loading {}", config.display()))?;
    if notify_telegram {
        validate_telegram_notify(&telegram_token_file, &telegram_chat_id_cache)?;
    }
    if let Some(source_ref) = source_ref {
        config.source_archive.ref_name = source_ref;
    }
    config.deploy.enabled = !no_deploy && (deploy || config.deploy.enabled);
    config.deploy.dry_run = resolve_deploy_dry_run(config.deploy.dry_run, deploy, dry_run);

    let prompt_text = fs::read_to_string(&prompt_file)
        .with_context(|| format!("reading prompt file {}", prompt_file.display()))?;
    let artifacts_dir = artifacts_dir.unwrap_or_else(|| PathBuf::from(&config.paths.artifacts_dir));
    let downloads_dir = path_arg_or_env_or_default(
        downloads_dir,
        &config.paths.downloads_dir_env,
        artifacts_dir.join("downloads"),
    )?;
    let profile_dir = path_arg_or_env_or_default(
        profile_dir,
        &config.browser.profile_dir_env,
        default_managed_chrome_profile_dir(),
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
    let bridge_cmd = bridge_command(bridge_cmd)?;
    let run_id = run_id.unwrap_or_else(default_run_id);
    let opts = jailgun_orchestrator::RunOptions {
        run_id,
        config,
        prompt_text,
        tabs_override: tabs,
        no_deploy,
        dry_run,
        profile_dir,
        downloads_dir,
        artifacts_dir,
        bridge_cmd,
        bridge_env,
        repo_url,
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
        max_runtime_seconds: None,
        event_buffer,
        deploy_concurrency,
    };
    let handle = run_orchestration(opts).await?;
    if notify_telegram {
        tokio::spawn(jailgun_notify::run_telegram_subscriber(
            handle.events_rx.resubscribe(),
            telegram_token_file,
            telegram_chat_id_cache,
        ));
    }
    stream_run(handle).await?;
    Ok(())
}

async fn review_packet(
    summary_json: PathBuf,
    base: String,
    head: String,
    repo: PathBuf,
    output: Option<PathBuf>,
    patch_bytes: usize,
) -> Result<()> {
    let packet = agent::build_review_packet(&summary_json, &repo, &base, &head, patch_bytes)?;
    let bytes = serde_json::to_vec_pretty(&packet)?;
    if let Some(output) = output {
        ensure_parent_dir(&output)?;
        fs::write(&output, bytes).with_context(|| format!("writing {}", output.display()))?;
    } else {
        println!("{}", String::from_utf8_lossy(&bytes));
    }
    Ok(())
}

async fn telegram_send(
    token_file: PathBuf,
    chat_id_cache: PathBuf,
    chat_id: Option<String>,
    message: String,
) -> Result<()> {
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
    Ok(())
}

async fn notify_commit(
    token_file: PathBuf,
    chat_id_cache: PathBuf,
    chat_id: Option<String>,
    repo: PathBuf,
    revision: String,
) -> Result<()> {
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
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn serve(
    config: PathBuf,
    addr: std::net::SocketAddr,
    dashboard_dist: Option<PathBuf>,
    live: bool,
    ingest_token: Option<String>,
    notify_telegram: bool,
    telegram_token_file: PathBuf,
    telegram_chat_id_cache: PathBuf,
) -> Result<()> {
    let config = JailgunConfig::from_toml_path(&config)
        .with_context(|| format!("loading {}", config.display()))?;
    let receipt_dir = PathBuf::from(&config.paths.artifacts_dir).join("receipts");
    let state = if live {
        let (state, rx) = AppState::live(config, receipt_dir, 1024);
        if notify_telegram {
            validate_telegram_notify(&telegram_token_file, &telegram_chat_id_cache)?;
            tokio::spawn(jailgun_notify::run_telegram_subscriber(
                rx,
                telegram_token_file,
                telegram_chat_id_cache,
            ));
        }
        state.with_ingest_token(ingest_token)
    } else {
        if notify_telegram {
            anyhow::bail!("--notify-telegram requires --live");
        }
        AppState::fixture(config)
    };
    let router = match dashboard_dist {
        Some(dir) => router_with_static(state, dir),
        None => api_router(state),
    };
    println!("listening on http://{addr} (live={live} notify_telegram={notify_telegram})");
    jailgun_server::serve(addr, router).await?;
    Ok(())
}

async fn fixture(kind: FixtureKind) -> Result<()> {
    let config = JailgunConfig::default();
    let state = AppState::fixture(config);
    match kind {
        FixtureKind::Runs => println!(
            "{}",
            serde_json::to_string_pretty(&state.runs.read().await.clone())?
        ),
        FixtureKind::Config => println!(
            "{}",
            serde_json::to_string_pretty(&state.config.redacted_for_display())?
        ),
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

fn validate_telegram_notify(token_file: &Path, chat_id_cache: &Path) -> Result<()> {
    let mut config = TelegramConfig::from_token_file(token_file)
        .with_context(|| format!("loading {}", token_file.display()))?;
    if config.chat_id.is_none() {
        config.chat_id = read_chat_id_cache(chat_id_cache)?;
    }
    if config
        .chat_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        anyhow::bail!(
            "--notify-telegram requires a chat id in {} or {}",
            token_file.display(),
            chat_id_cache.display()
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

fn parse_env_overrides(values: Vec<String>) -> Result<std::collections::BTreeMap<String, String>> {
    let mut envs = std::collections::BTreeMap::new();
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

async fn stream_run(mut handle: jailgun_orchestrator::OrchestratorHandle) -> Result<()> {
    let mut events_open = true;
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
            summary = &mut handle.completion => {
                let summary = summary.context("orchestrator task ended before sending a summary")?;
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "type": "run-summary",
                        "summary": summary,
                    }))?
                );
                break;
            }
        }
    }
    let _ = handle.shutdown.send(true);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jailgun_core::TarValidation;
    use jailgun_deploy::DeployOutcome;

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
    fn github_repo_inference_stays_within_owner_repo_boundary() {
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
        assert_eq!(
            infer_github_repo("https://notgithub.com/example/repo.git"),
            None
        );
        assert_eq!(
            infer_github_repo("https://github.com/example/repo/tree/main"),
            None
        );
    }
}

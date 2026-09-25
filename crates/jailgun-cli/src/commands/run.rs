use std::{env, fs, path::PathBuf};

use anyhow::{Context, Result};
use jailgun_core::{validate_run_id, JailgunConfig};
use jailgun_orchestrator::{
    run_orchestration,
    support::{
        arg_or_env, bridge_command, default_managed_chrome_profile_dir,
        default_managed_chrome_state_dir, default_run_id, deploy_remote_command, infer_github_repo,
        path_arg_or_env_or_default,
    },
};

use crate::commands::{deploy::resolve_deploy_dry_run, telegram::validate_telegram_notify};

#[allow(clippy::too_many_arguments)]
pub(super) async fn run(
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
    let artifacts_dir = match artifacts_dir {
        Some(artifacts_dir) => artifacts_dir,
        None => PathBuf::from(&config.paths.artifacts_dir),
    };
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
    let repo_url = match source_repo_url {
        Some(repo_url) => repo_url,
        None => match env::var(&config.source_archive.repo_url_env) {
            Ok(repo_url) => repo_url,
            Err(_) => config.project.repository.clone(),
        },
    };
    let ci_repo = match ci_repo {
        Some(ci_repo) => Some(ci_repo),
        None => infer_github_repo(&repo_url),
    };
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
    let run_id = validated_run_id(run_id)?;
    let opts = jailgun_orchestrator::RunOptions {
        run_id,
        config,
        prompt_text,
        tabs_override: tabs,
        no_deploy,
        dry_run,
        profile_dir: profile_dir.clone(),
        profile_pool: Vec::new(),
        tab_profile_dirs: Default::default(),
        downloads_dir,
        artifacts_dir,
        bridge_cmd,
        bridge_env,
        repo_url,
        local_archive_path: None,
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

pub(super) fn validated_run_id(run_id: Option<String>) -> Result<String> {
    let run_id = match run_id {
        Some(run_id) => run_id,
        None => default_run_id(),
    };
    validate_run_id(&run_id).map_err(anyhow::Error::msg)?;
    Ok(run_id)
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

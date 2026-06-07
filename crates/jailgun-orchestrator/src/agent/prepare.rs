use std::{
    env, fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use jailgun_core::{BrowserLeaseRequest, JailgunAgentRunRequest, JailgunConfig};

use crate::{
    agent::{
        accounts::{browser_registry_path_for_request, resolve_requested_accounts},
        execute::execute_prepared_agent_run,
        prepare_env::{apply_profile_env, clear_profile_routing_env},
        timestamp_now, AgentRunPaths, DefaultAgentRunBackend, NoopAgentRunEventSink,
        PreparedAgentRun, PreparedBrowserLease,
    },
    config::RunOptions,
    support::{
        arg_or_env, bridge_command, default_managed_chrome_profile_dir, default_run_id,
        deploy_remote_command, infer_github_repo, path_arg_or_env_or_default,
    },
};

pub async fn run_agent(
    request_path: String,
    events_jsonl: PathBuf,
    summary_json: PathBuf,
) -> Result<()> {
    let request = read_agent_request(&request_path)?;
    let prepared = prepare_agent_run(
        request,
        AgentRunPaths {
            events_jsonl,
            summary_json,
        },
    )?;
    let summary =
        execute_prepared_agent_run(prepared, &DefaultAgentRunBackend, &NoopAgentRunEventSink)
            .await?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    if summary.status != "succeeded" {
        anyhow::bail!(
            "agent run {} finished with status {}",
            summary.run_id,
            summary.status
        );
    }
    Ok(())
}

pub fn prepare_agent_run(
    request: JailgunAgentRunRequest,
    output_paths: AgentRunPaths,
) -> Result<PreparedAgentRun> {
    let config_path = request
        .config_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("config/jailgun.example.toml"));
    let resolved_config_path = resolve_config_path(&config_path);
    let mut config = JailgunConfig::from_toml_path(&resolved_config_path)
        .with_context(|| format!("loading {}", config_path.display()))?;
    request
        .validate_for_config_tabs(config.browser.tabs)
        .map_err(anyhow::Error::msg)?;

    let tabs = request
        .effective_tabs(config.browser.tabs)
        .map_err(anyhow::Error::msg)?;
    let max_runtime_seconds = request
        .effective_max_runtime_seconds()
        .map_err(anyhow::Error::msg)?;

    apply_request_config_overrides(&request, &mut config)?;

    let account_profiles = resolve_requested_accounts(&request, &config, tabs)?;
    let uses_browser_lease = !account_profiles.is_empty();
    let explicit_profile_pool = if uses_browser_lease {
        Vec::new()
    } else {
        request.browser.profile_pool.clone()
    };

    let prompt_text = fs::read_to_string(&request.prompt_file)
        .with_context(|| format!("reading prompt file {}", request.prompt_file.display()))?;
    let artifacts_dir = request
        .browser
        .artifacts_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from(&config.paths.artifacts_dir));
    let downloads_dir = path_arg_or_env_or_default(
        request.browser.downloads_dir.clone(),
        &config.paths.downloads_dir_env,
        artifacts_dir.join("downloads"),
    )?;
    let requested_profile_dir = if account_profiles.is_empty() {
        request
            .browser
            .profile_dir
            .clone()
            .or_else(|| explicit_profile_pool.first().cloned())
    } else {
        explicit_profile_pool.first().cloned()
    };
    let profile_dir = path_arg_or_env_or_default(
        requested_profile_dir,
        &config.browser.profile_dir_env,
        default_managed_chrome_profile_dir(),
    )?;
    let repo_url = request
        .source_archive
        .repo_url
        .clone()
        .or_else(|| request.repo.repository.clone())
        .or_else(|| env::var(&config.source_archive.repo_url_env).ok())
        .unwrap_or_else(|| config.project.repository.clone());
    let ci_repo = request
        .ci
        .repo
        .clone()
        .or_else(|| infer_github_repo(&repo_url));
    let deploy_remote_host = deploy_value(
        config.deploy.enabled,
        request.deploy.remote_host.clone(),
        &config.deploy.remote_host_env,
        "remote host",
    )?;
    let deploy_remote_dir = deploy_value(
        config.deploy.enabled,
        request.deploy.remote_dir.clone(),
        &config.deploy.remote_dir_env,
        "remote dir",
    )?;
    let deploy_remote_command = if config.deploy.enabled {
        Some(deploy_remote_command(
            request.deploy.remote_command.clone(),
            &config.deploy.remote_command_env,
        )?)
    } else {
        None
    };

    let mut bridge_env = request.browser.bridge_env.clone();
    bridge_env.insert(
        "JAILGUN_DOWNLOADS_DIR".into(),
        downloads_dir.display().to_string(),
    );
    bridge_env.insert(
        "JAILGUN_ARTIFACTS_DIR".into(),
        artifacts_dir.display().to_string(),
    );
    if let Some(tar_target_name) = request.source_archive.tar_target_name.as_ref() {
        bridge_env.insert("JAILGUN_TAR_TARGET_NAME".into(), tar_target_name.clone());
    }
    if let Some(download_target_name) = request.browser.download_target_name.as_ref() {
        if !download_target_name.trim().is_empty() {
            bridge_env.insert(
                "JAILGUN_DOWNLOAD_TARGET_NAME".into(),
                download_target_name.clone(),
            );
        }
    }
    if uses_browser_lease {
        clear_profile_routing_env(&mut bridge_env, &config);
    } else {
        apply_profile_env(
            &mut bridge_env,
            &config,
            &[],
            &explicit_profile_pool,
            &profile_dir,
        )?;
    }

    let bridge_cmd = bridge_command(request.browser.bridge_cmd.clone())?;
    let run_id = request.run_id.clone().unwrap_or_else(default_run_id);
    let started_at = timestamp_now();
    let deploy_expected_top_level = request
        .deploy
        .expected_top_level
        .clone()
        .or_else(|| request.source_archive.expected_top_level.clone());
    let opts = RunOptions {
        run_id: run_id.clone(),
        config: config.clone(),
        prompt_text: prompt_text.clone(),
        tabs_override: Some(tabs),
        no_deploy: !config.deploy.enabled,
        dry_run: config.deploy.dry_run,
        profile_dir: profile_dir.clone(),
        profile_pool: explicit_profile_pool.clone(),
        tab_profile_dirs: Default::default(),
        downloads_dir,
        artifacts_dir,
        bridge_cmd,
        bridge_env,
        repo_url: repo_url.clone(),
        local_archive_path: None,
        deploy_remote_host,
        deploy_remote_dir,
        deploy_remote_command,
        deploy_expected_top_level: deploy_expected_top_level.clone(),
        ci_tracker_enabled: request.ci.enabled,
        ci_repo,
        ci_branch: request.ci.branch.clone().unwrap_or_else(|| "main".into()),
        ci_max_attempts: request.ci.max_attempts.unwrap_or(20),
        ci_poll_seconds: request.ci.poll_seconds.unwrap_or(30),
        status_max_minutes: 30,
        max_runtime_seconds: Some(max_runtime_seconds),
        event_buffer: request.browser.event_buffer.unwrap_or(1024),
        deploy_concurrency: request.browser.deploy_concurrency.unwrap_or(1),
    };
    let browser_lease = uses_browser_lease.then(|| PreparedBrowserLease {
        registry_path: browser_registry_path_for_request(&request, &config),
        request: BrowserLeaseRequest {
            run_id: run_id.clone(),
            account_ids: request.browser.account_ids.clone(),
            tabs,
            allow_queueing: request.browser.allow_queueing,
            queue_timeout_seconds: request
                .effective_browser_queue_timeout_seconds()
                .unwrap_or(jailgun_core::DEFAULT_BROWSER_QUEUE_TIMEOUT_SECONDS),
            lease_ttl_seconds: max_runtime_seconds.saturating_add(600),
        },
    });

    Ok(PreparedAgentRun {
        request,
        config,
        repo_url,
        tabs,
        max_runtime_seconds,
        deploy_expected_top_level,
        started_at,
        prompt_text,
        output_paths,
        opts,
        browser_lease,
    })
}

fn apply_request_config_overrides(
    request: &JailgunAgentRunRequest,
    config: &mut JailgunConfig,
) -> Result<()> {
    if let Some(enabled) = request.source_archive.enabled {
        config.source_archive.enabled = enabled;
    }
    if let Some(ref_name) = request
        .source_archive
        .ref_name
        .clone()
        .or_else(|| request.repo.ref_name.clone())
    {
        config.source_archive.ref_name = ref_name;
    }
    config.deploy.enabled = request.deploy.enabled;
    config.deploy.dry_run = !request.deploy.enabled || request.deploy.dry_run;
    config.prompt_policy.deny_github_write_by_default = !request.github.allow_write_prompts;
    config.prompt_policy.allow_write_prompts = request.github.allow_write_prompts;
    config.prompt_policy.allow_info_prompts = request.github.allow_info_prompts;
    if !request.github.allowed_repositories.is_empty() {
        config.prompt_policy.allowed_repositories = request.github.allowed_repositories.clone();
    }
    config
        .validate()
        .context("validating agent-adjusted config")
}

fn deploy_value(
    enabled: bool,
    value: Option<String>,
    env_name: &str,
    label: &str,
) -> Result<Option<String>> {
    if enabled {
        Ok(Some(arg_or_env(value, env_name, label)?))
    } else {
        Ok(None)
    }
}

fn resolve_config_path(path: &Path) -> PathBuf {
    if path.is_absolute() || path.exists() {
        return path.to_path_buf();
    }
    let workspace_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(path);
    if workspace_path.exists() {
        workspace_path
    } else {
        path.to_path_buf()
    }
}

fn read_agent_request(path: &str) -> Result<JailgunAgentRunRequest> {
    let text = if path == "-" {
        let mut text = String::new();
        io::stdin()
            .read_to_string(&mut text)
            .context("reading agent request from stdin")?;
        text
    } else {
        fs::read_to_string(path).with_context(|| format!("reading agent request {path}"))?
    };
    serde_json::from_str(&text).context("parsing agent request JSON")
}

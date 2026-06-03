use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use async_trait::async_trait;
use jailgun_core::{
    derive_changed_file_paths, validate_tar_gz, EventKind, JailgunAgentRunRequest,
    JailgunAgentRunSummary, JailgunArtifact, JailgunChangedFile, JailgunConfig, JailgunEvent,
    JailgunFailure, JailgunReviewPacket, JailgunSourceArchiveSummary,
    JAILGUN_AGENT_INTERFACE_VERSION,
};
use tokio::io::AsyncWriteExt;

use crate::{
    config::RunOptions,
    run::{run_orchestration, OrchestratorHandle, RunSummary},
    support::{
        arg_or_env, bridge_command, default_managed_chrome_profile_dir,
        default_managed_chrome_state_dir, default_run_id, deploy_remote_command, ensure_parent_dir,
        infer_github_repo, path_arg_or_env_or_default, profile_pool_env_value,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunPaths {
    pub events_jsonl: PathBuf,
    pub summary_json: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PreparedAgentRun {
    pub request: JailgunAgentRunRequest,
    pub config: JailgunConfig,
    pub repo_url: String,
    pub tabs: u16,
    pub max_runtime_seconds: u64,
    pub deploy_expected_top_level: Option<String>,
    pub started_at: String,
    pub prompt_text: String,
    pub output_paths: AgentRunPaths,
    pub opts: RunOptions,
}

#[async_trait]
pub trait AgentRunBackend: Send + Sync {
    async fn start(&self, opts: RunOptions) -> Result<OrchestratorHandle>;
}

pub struct DefaultAgentRunBackend;

#[async_trait]
impl AgentRunBackend for DefaultAgentRunBackend {
    async fn start(&self, opts: RunOptions) -> Result<OrchestratorHandle> {
        Ok(run_orchestration(opts).await?)
    }
}

#[async_trait]
pub trait AgentRunEventSink: Send + Sync {
    async fn on_event(&self, _event: &JailgunEvent) -> Result<()> {
        Ok(())
    }

    async fn on_summary(&self, _summary: &JailgunAgentRunSummary) -> Result<()> {
        Ok(())
    }
}

pub struct NoopAgentRunEventSink;

#[async_trait]
impl AgentRunEventSink for NoopAgentRunEventSink {}

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
    config.prompt_policy.allow_info_prompts = request.github.allow_info_prompts;
    if !request.github.allowed_repositories.is_empty() {
        config.prompt_policy.allowed_repositories = request.github.allowed_repositories.clone();
    }
    config
        .validate()
        .context("validating agent-adjusted config")?;

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
    let profile_pool = request.browser.profile_pool.clone();
    let profile_dir = path_arg_or_env_or_default(
        request.browser.profile_dir.clone(),
        &config.browser.profile_dir_env,
        profile_pool
            .first()
            .cloned()
            .unwrap_or_else(default_managed_chrome_profile_dir),
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
    let deploy_remote_host = if config.deploy.enabled {
        Some(arg_or_env(
            request.deploy.remote_host.clone(),
            &config.deploy.remote_host_env,
            "remote host",
        )?)
    } else {
        None
    };
    let deploy_remote_dir = if config.deploy.enabled {
        Some(arg_or_env(
            request.deploy.remote_dir.clone(),
            &config.deploy.remote_dir_env,
            "remote dir",
        )?)
    } else {
        None
    };
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
        initial_tab_burst: request.browser.initial_tab_burst,
        loop_count: 0,
        no_deploy: !config.deploy.enabled,
        dry_run: config.deploy.dry_run,
        profile_dir,
        profile_pool,
        downloads_dir,
        artifacts_dir,
        bridge_cmd,
        bridge_env,
        repo_url: repo_url.clone(),
        fresh_source_clone: request.source_archive.fresh_source_clone,
        deploy_remote_host,
        deploy_remote_dir,
        deploy_remote_command,
        deploy_expected_top_level: deploy_expected_top_level.clone(),
        ci_tracker_enabled: request.ci.enabled,
        ci_repo,
        ci_branch: request.ci.branch.clone().unwrap_or_else(|| "main".into()),
        ci_max_attempts: request.ci.max_attempts.unwrap_or(20),
        ci_poll_seconds: request.ci.poll_seconds.unwrap_or(30),
        status_max_minutes: max_runtime_minutes(max_runtime_seconds),
        event_buffer: request.browser.event_buffer.unwrap_or(1024),
        deploy_concurrency: request.browser.deploy_concurrency.unwrap_or(1),
    };

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
    })
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

fn max_runtime_minutes(seconds: u64) -> u16 {
    let minutes = seconds.saturating_add(59) / 60;
    minutes.clamp(1, u16::MAX as u64) as u16
}

pub async fn execute_prepared_agent_run(
    prepared: PreparedAgentRun,
    backend: &dyn AgentRunBackend,
    sink: &dyn AgentRunEventSink,
) -> Result<JailgunAgentRunSummary> {
    let handle = backend.start(prepared.opts.clone()).await?;
    let collection = collect_agent_run_events(
        handle,
        &prepared.output_paths.events_jsonl,
        &prepared.opts.run_id,
        prepared.tabs,
        prepared.max_runtime_seconds,
        sink,
    )
    .await?;
    let finished_at = timestamp_now();
    let summary = build_agent_summary(
        &prepared.request,
        &prepared.config,
        &prepared.repo_url,
        &prepared.output_paths.events_jsonl,
        prepared.started_at,
        finished_at,
        prepared.tabs,
        prepared.max_runtime_seconds,
        prepared.deploy_expected_top_level.as_deref(),
        collection,
    );
    ensure_parent_dir(&prepared.output_paths.summary_json)?;
    fs::write(
        &prepared.output_paths.summary_json,
        serde_json::to_vec_pretty(&summary)?,
    )
    .with_context(|| format!("writing {}", prepared.output_paths.summary_json.display()))?;
    sink.on_summary(&summary).await?;
    Ok(summary)
}

struct AgentRunCollection {
    summary: RunSummary,
    events: Vec<JailgunEvent>,
    timed_out: bool,
}

async fn collect_agent_run_events(
    mut handle: OrchestratorHandle,
    events_jsonl: &Path,
    run_id: &str,
    tabs: u16,
    max_runtime_seconds: u64,
    sink: &dyn AgentRunEventSink,
) -> Result<AgentRunCollection> {
    ensure_parent_dir(events_jsonl)?;
    let mut file = tokio::fs::File::create(events_jsonl)
        .await
        .with_context(|| format!("creating {}", events_jsonl.display()))?;
    let mut events = Vec::new();
    let mut events_open = true;
    let deadline = tokio::time::sleep(std::time::Duration::from_secs(max_runtime_seconds));
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            _ = &mut deadline => {
                let _ = handle.shutdown.send(true);
                file.flush().await?;
                return Ok(AgentRunCollection {
                    summary: RunSummary {
                        run_id: run_id.to_string(),
                        batch_tabs: tabs,
                        loop_count: 0,
                        planned_tabs: tabs,
                        total_tabs: tabs,
                        downloaded: 0,
                        deployed: 0,
                        failures: vec![(0, "agent max runtime exceeded".into())],
                        denied_github_prompts: 0,
                        allowed_info_prompts: 0,
                        early_stops_succeeded: 0,
                        early_stops_attempted: 0,
                    },
                    events,
                    timed_out: true,
                });
            }
            event = handle.events_rx.recv(), if events_open => {
                match event {
                    Ok(event) => {
                        file.write_all(&serde_json::to_vec(&event)?).await?;
                        file.write_all(b"\n").await?;
                        sink.on_event(&event).await?;
                        events.push(event);
                    }
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
                file.flush().await?;
                let _ = handle.shutdown.send(true);
                return Ok(AgentRunCollection {
                    summary,
                    events,
                    timed_out: false,
                });
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_agent_summary(
    request: &JailgunAgentRunRequest,
    config: &JailgunConfig,
    repo_url: &str,
    events_jsonl: &Path,
    started_at: String,
    finished_at: String,
    tab_count: u16,
    max_runtime_seconds: u64,
    expected_top_level: Option<&str>,
    collection: AgentRunCollection,
) -> JailgunAgentRunSummary {
    let mut failures = collection
        .summary
        .failures
        .iter()
        .map(|(tab_id, message)| JailgunFailure {
            tab_id: (*tab_id != 0).then_some(*tab_id),
            code: "orchestrator".into(),
            message: message.clone(),
        })
        .collect::<Vec<_>>();
    let (artifacts, artifact_failures) =
        artifacts_from_events(&collection.events, config, expected_top_level);
    failures.extend(artifact_failures);
    let changed_files = artifacts
        .iter()
        .flat_map(|artifact| artifact.changed_files.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let receipt_paths = receipt_paths_from_events(&collection.events);
    let deploy_status = deploy_status_from_events(&collection.events, config.deploy.enabled);
    let ci_status = ci_status_from_events(&collection.events, request.ci.enabled);
    let status = if collection.timed_out {
        "timed-out"
    } else if failures.is_empty() {
        "succeeded"
    } else {
        "failed"
    };
    let mut repo_ref = request.repo.clone();
    if repo_ref.repository.is_none() {
        repo_ref.repository = Some(repo_url.to_string());
    }
    if repo_ref.ref_name.is_none() {
        repo_ref.ref_name = Some(config.source_archive.ref_name.clone());
    }

    JailgunAgentRunSummary {
        version: JAILGUN_AGENT_INTERFACE_VERSION,
        run_id: collection.summary.run_id,
        status: status.into(),
        prompt_ref: request.prompt_ref.clone(),
        tab_count,
        batch_tabs: collection.summary.batch_tabs,
        loop_count: collection.summary.loop_count,
        planned_tabs: collection.summary.planned_tabs,
        max_runtime_seconds,
        repo_ref,
        source_archive: JailgunSourceArchiveSummary {
            enabled: config.source_archive.enabled,
            repo_url: repo_url.to_string(),
            ref_name: config.source_archive.ref_name.clone(),
            prefix: config.source_archive.prefix.clone(),
            archive_filename: config.source_archive.archive_filename.clone(),
        },
        deploy_status,
        ci_status,
        changed_files,
        artifacts,
        failures,
        events_jsonl: events_jsonl.to_path_buf(),
        receipt_paths,
        started_at,
        finished_at,
        denied_github_prompts: collection.summary.denied_github_prompts,
        allowed_info_prompts: collection.summary.allowed_info_prompts,
        early_stops_succeeded: collection.summary.early_stops_succeeded,
        early_stops_attempted: collection.summary.early_stops_attempted,
        github_write_prompts_allowed: request.github.allow_write_prompts,
    }
}

fn artifacts_from_events(
    events: &[JailgunEvent],
    config: &JailgunConfig,
    expected_top_level: Option<&str>,
) -> (Vec<JailgunArtifact>, Vec<JailgunFailure>) {
    let mut artifacts = Vec::new();
    let mut failures = Vec::new();
    let require_single_top_level =
        config.deploy.remote_strip_components > 0 || expected_top_level.is_some();
    for event in events
        .iter()
        .filter(|event| matches!(event.kind, EventKind::DownloadReceipt))
    {
        let Some(path) = event.fields.get("local_path") else {
            continue;
        };
        let archive_path = PathBuf::from(path);
        let validation = match validate_tar_gz(&archive_path, require_single_top_level) {
            Ok(validation) => {
                if let Some(expected) = expected_top_level {
                    if validation.top_level.as_deref() != Some(expected) {
                        failures.push(JailgunFailure {
                            tab_id: event.tab_id,
                            code: "tar-validation".into(),
                            message: format!(
                                "archive top-level must be {expected}/, found {}",
                                validation.top_level.as_deref().unwrap_or("(multiple)")
                            ),
                        });
                    }
                }
                Some(validation)
            }
            Err(error) => {
                failures.push(JailgunFailure {
                    tab_id: event.tab_id,
                    code: "tar-validation".into(),
                    message: error.to_string(),
                });
                None
            }
        };
        let changed_files = validation
            .as_ref()
            .map(|validation| {
                derive_changed_file_paths(
                    validation,
                    config.deploy.remote_strip_components as usize,
                )
            })
            .unwrap_or_default();
        artifacts.push(JailgunArtifact {
            kind: "downloaded-archive".into(),
            path: archive_path,
            sha256: event.fields.get("sha256").cloned(),
            size_bytes: event
                .fields
                .get("size_bytes")
                .and_then(|value| value.parse::<u64>().ok()),
            receipt_path: event.fields.get("receipt_path").map(PathBuf::from),
            tar_validation: validation,
            changed_files,
        });
    }
    (artifacts, failures)
}

fn receipt_paths_from_events(events: &[JailgunEvent]) -> Vec<PathBuf> {
    events
        .iter()
        .filter_map(|event| event.fields.get("receipt_path").map(PathBuf::from))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn deploy_status_from_events(events: &[JailgunEvent], deploy_enabled: bool) -> String {
    if !deploy_enabled {
        return "disabled".into();
    }
    events
        .iter()
        .rev()
        .find(|event| matches!(event.kind, EventKind::DeployFinished))
        .and_then(|event| event.fields.get("outcome").cloned())
        .unwrap_or_else(|| "not-finished".into())
}

fn ci_status_from_events(events: &[JailgunEvent], ci_enabled: bool) -> String {
    if !ci_enabled {
        return "disabled".into();
    }
    events
        .iter()
        .rev()
        .find_map(|event| event.fields.get("ci_state").cloned())
        .unwrap_or_else(|| "unknown".into())
}

pub fn build_review_packet(
    summary_json: &Path,
    repo: &Path,
    base: &str,
    head: &str,
    patch_bytes: usize,
) -> Result<JailgunReviewPacket> {
    let summary_text = fs::read_to_string(summary_json)
        .with_context(|| format!("reading {}", summary_json.display()))?;
    let summary: JailgunAgentRunSummary =
        serde_json::from_str(&summary_text).context("parsing run summary JSON")?;
    let base_sha = git_output(repo, &["rev-parse", base])?;
    let head_sha = git_output(repo, &["rev-parse", head])?;
    let base_sha = base_sha.trim().to_string();
    let head_sha = head_sha.trim().to_string();
    let diff_stat = git_output(
        repo,
        &[
            "diff",
            "--stat",
            "--find-renames",
            base_sha.as_str(),
            head_sha.as_str(),
        ],
    )?;
    let name_status_text = git_output(
        repo,
        &[
            "diff",
            "--name-status",
            "--find-renames",
            base_sha.as_str(),
            head_sha.as_str(),
        ],
    )?;
    let patch = cap_utf8(
        git_output(
            repo,
            &[
                "diff",
                "--no-ext-diff",
                "--find-renames",
                "--unified=80",
                base_sha.as_str(),
                head_sha.as_str(),
            ],
        )?,
        patch_bytes,
    );
    let name_status = parse_name_status(&name_status_text);
    let changed_tests = name_status
        .iter()
        .filter_map(|file| is_test_path(&file.path).then_some(file.path.clone()))
        .collect::<Vec<_>>();
    let mut source_metadata = BTreeMap::new();
    source_metadata.insert("repo_path".into(), repo.display().to_string());
    source_metadata.insert("summary_json".into(), summary_json.display().to_string());
    source_metadata.insert(
        "events_jsonl".into(),
        summary.events_jsonl.display().to_string(),
    );
    source_metadata.insert(
        "interface_version".into(),
        JAILGUN_AGENT_INTERFACE_VERSION.to_string(),
    );

    Ok(JailgunReviewPacket {
        version: JAILGUN_AGENT_INTERFACE_VERSION,
        generated_at: timestamp_now(),
        run_id: summary.run_id.clone(),
        prompt_ref: summary.prompt_ref.clone(),
        base_sha,
        head_sha,
        diff_stat,
        name_status,
        patch,
        changed_tests,
        artifacts: summary.artifacts.clone(),
        receipt_paths: summary.receipt_paths.clone(),
        summary,
        source_metadata,
    })
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

fn git_output(repo: &Path, args: &[&str]) -> Result<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .with_context(|| format!("running git {}", args.join(" ")))?;
    if !output.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_name_status(text: &str) -> Vec<JailgunChangedFile> {
    text.lines()
        .filter_map(|line| {
            let parts = line.split('\t').collect::<Vec<_>>();
            let status = parts.first()?.to_string();
            if status.starts_with('R') || status.starts_with('C') {
                Some(JailgunChangedFile {
                    status,
                    old_path: parts.get(1).map(|value| (*value).to_string()),
                    path: parts.get(2)?.to_string(),
                })
            } else {
                Some(JailgunChangedFile {
                    status,
                    path: parts.get(1)?.to_string(),
                    old_path: None,
                })
            }
        })
        .collect()
}

fn is_test_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.starts_with("tests/")
        || lower.contains("/tests/")
        || lower.contains("__tests__")
        || lower.ends_with("_test.rs")
        || lower.ends_with("_tests.rs")
        || lower.ends_with(".test.ts")
        || lower.ends_with(".test.tsx")
        || lower.ends_with(".spec.ts")
        || lower.ends_with(".spec.tsx")
        || lower.ends_with(".test.mjs")
}

fn cap_utf8(mut text: String, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text;
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    text.truncate(boundary);
    text.push_str(&format!("\n[patch truncated at {max_bytes} bytes]\n"));
    text
}

fn timestamp_now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request_with_prompt(prompt_file: PathBuf) -> JailgunAgentRunRequest {
        let mut request = JailgunAgentRunRequest {
            version: JAILGUN_AGENT_INTERFACE_VERSION,
            run_id: Some("agent-run-1".into()),
            prompt_ref: "jmcp://prompt/1".into(),
            prompt_file,
            config_path: None,
            tabs: Some(2),
            max_runtime_seconds: Some(125),
            repo: Default::default(),
            source_archive: Default::default(),
            deploy: Default::default(),
            ci: Default::default(),
            browser: Default::default(),
            github: Default::default(),
        };
        request.browser.bridge_cmd = vec!["fake-bridge".into()];
        request
    }

    #[test]
    fn prepare_agent_run_preserves_current_run_options() {
        let temp = tempfile::tempdir().expect("tempdir");
        let prompt_file = temp.path().join("prompt.txt");
        fs::write(&prompt_file, "ship the change").expect("write prompt");
        let profile_a = temp.path().join("profile-a");
        let profile_b = temp.path().join("profile-b");
        let mut request = request_with_prompt(prompt_file);
        request.browser.initial_tab_burst = Some(2);
        request.browser.profile_pool = vec![profile_a.clone(), profile_b.clone()];
        request.browser.event_buffer = Some(77);
        request.browser.deploy_concurrency = Some(3);
        request.source_archive.fresh_source_clone = true;

        let prepared = prepare_agent_run(
            request,
            AgentRunPaths {
                events_jsonl: temp.path().join("events.jsonl"),
                summary_json: temp.path().join("summary.json"),
            },
        )
        .expect("prepare agent run");

        assert_eq!(prepared.opts.tabs_override, Some(2));
        assert_eq!(prepared.opts.initial_tab_burst, Some(2));
        assert_eq!(prepared.opts.loop_count, 0);
        assert_eq!(prepared.opts.profile_pool, vec![profile_a, profile_b]);
        assert!(prepared.opts.fresh_source_clone);
        assert_eq!(prepared.opts.event_buffer, 77);
        assert_eq!(prepared.opts.deploy_concurrency, 3);
        assert_eq!(prepared.opts.status_max_minutes, 3);
        let pool_env = prepared
            .opts
            .bridge_env
            .get("JAILGUN_CHROME_PROFILE_POOL")
            .expect("profile pool env");
        assert_eq!(
            std::env::split_paths(pool_env).collect::<Vec<_>>(),
            prepared.opts.profile_pool
        );
    }

    #[test]
    fn review_packet_helpers_parse_name_status_and_tests() {
        let files = parse_name_status("M\tcrates/lib.rs\nR100\told.test.ts\tnew.test.ts\n");

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].status, "M");
        assert_eq!(files[0].path, "crates/lib.rs");
        assert_eq!(files[1].status, "R100");
        assert_eq!(files[1].old_path.as_deref(), Some("old.test.ts"));
        assert_eq!(files[1].path, "new.test.ts");
        assert!(is_test_path(&files[1].path));
        assert!(!is_test_path(&files[0].path));
    }

    #[test]
    fn caps_review_patch_on_utf8_boundary() {
        let capped = cap_utf8("abcédef".into(), 4);

        assert!(capped.starts_with("abc"));
        assert!(capped.contains("patch truncated at 4 bytes"));
    }

    #[test]
    fn summary_helpers_extract_deploy_ci_and_receipts() {
        let event = JailgunEvent::new("run-1", EventKind::DeployFinished, "deploy finished")
            .with_field("outcome", "dry-run-staged")
            .with_field("ci_state", "skipped")
            .with_field("receipt_path", "target/receipts/deploy.json");
        let events = vec![event];

        assert_eq!(deploy_status_from_events(&events, true), "dry-run-staged");
        assert_eq!(ci_status_from_events(&events, true), "skipped");
        assert_eq!(
            receipt_paths_from_events(&events),
            vec![PathBuf::from("target/receipts/deploy.json")]
        );
    }
}

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use jailgun_core::{
    derive_changed_file_paths, validate_tar_gz, EventKind, JailgunAgentRunRequest,
    JailgunAgentRunSummary, JailgunArtifact, JailgunConfig, JailgunEvent, JailgunFailure,
    JailgunSourceArchiveSummary, JAILGUN_AGENT_INTERFACE_VERSION,
};

use super::execute::AgentRunCollection;

#[allow(clippy::too_many_arguments)]
pub(super) fn build_agent_summary(
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

pub(super) fn receipt_paths_from_events(events: &[JailgunEvent]) -> Vec<PathBuf> {
    events
        .iter()
        .filter_map(|event| event.fields.get("receipt_path").map(PathBuf::from))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(super) fn deploy_status_from_events(events: &[JailgunEvent], deploy_enabled: bool) -> String {
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

pub(super) fn ci_status_from_events(events: &[JailgunEvent], ci_enabled: bool) -> String {
    if !ci_enabled {
        return "disabled".into();
    }
    events
        .iter()
        .rev()
        .find_map(|event| event.fields.get("ci_state").cloned())
        .unwrap_or_else(|| "unknown".into())
}

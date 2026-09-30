use std::process::Stdio;

use async_trait::async_trait;
use serde::Deserialize;
use tokio::process::Command;

use crate::{
    ci::{CiState, CiTracker},
    deploy::DeployError,
};

pub struct SshCiTracker {
    repo: Option<String>,
}

impl SshCiTracker {
    pub fn new() -> Self {
        Self { repo: None }
    }

    pub fn with_repo(repo: Option<String>) -> Self {
        Self {
            repo: repo.and_then(|value| {
                let value = value.trim().to_string();
                if value.is_empty() {
                    None
                } else {
                    Some(value)
                }
            }),
        }
    }

    fn gh_run_list_args(&self, commit_sha: &str, branch: &str) -> Vec<String> {
        let mut args = vec![
            "run".to_string(),
            "list".to_string(),
            "--branch".to_string(),
            branch.to_string(),
            "--commit".to_string(),
            commit_sha.to_string(),
            "--json".to_string(),
            "databaseId,status,conclusion,url".to_string(),
            "--limit".to_string(),
            "1".to_string(),
        ];
        self.append_repo_args(&mut args);
        args
    }

    fn gh_run_view_args(&self, run_id: &str) -> Vec<String> {
        let mut args = vec![
            "run".to_string(),
            "view".to_string(),
            run_id.to_string(),
            "--log-failed".to_string(),
        ];
        self.append_repo_args(&mut args);
        args
    }

    fn append_repo_args(&self, args: &mut Vec<String>) {
        if let Some(repo) = self.repo.as_ref() {
            args.push("--repo".to_string());
            args.push(repo.clone());
        }
    }
}

impl Default for SshCiTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CiTracker for SshCiTracker {
    async fn check(&mut self, commit_sha: &str, branch: &str) -> Result<CiState, DeployError> {
        let output = Command::new("gh")
            .args(self.gh_run_list_args(commit_sha, branch))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;
        let output = match output {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(CiState::Skipped {
                    reason: "gh-not-on-path".into(),
                });
            }
            Err(error) => {
                return Err(DeployError::CiTracker(format!(
                    "gh failed to start: {error}"
                )))
            }
        };
        if !output.status.success() {
            return Err(DeployError::CiTracker(format!(
                "gh run list exited {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        parse_gh_run_list(&output.stdout)
    }

    async fn capture_failure_log(
        &mut self,
        run_id: &str,
        max_bytes: usize,
    ) -> Result<String, DeployError> {
        let output = Command::new("gh")
            .args(self.gh_run_view_args(run_id))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;
        let output = match output {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
            Err(error) => {
                return Err(DeployError::CiTracker(format!(
                    "gh failed to start: {error}"
                )))
            }
        };
        if !output.status.success() {
            return Err(DeployError::CiTracker(format!(
                "gh run view exited {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        let mut text = String::from_utf8_lossy(&output.stdout).to_string();
        if text.len() > max_bytes {
            let start = text.len().saturating_sub(max_bytes);
            text = text[start..].to_string();
        }
        Ok(text)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhRun {
    database_id: Option<u64>,
    status: Option<String>,
    conclusion: Option<String>,
    url: Option<String>,
}

fn parse_gh_run_list(bytes: &[u8]) -> Result<CiState, DeployError> {
    let runs: Vec<GhRun> = serde_json::from_slice(bytes)?;
    let Some(run) = runs.into_iter().next() else {
        return Ok(CiState::Pending { run_id: None });
    };
    let run_id = run.database_id.map(|id| id.to_string()).unwrap_or_default();
    let url = run.url.unwrap_or_default();
    let status = run.status.unwrap_or_default();
    let conclusion = run.conclusion.unwrap_or_default();
    if status != "completed" {
        return Ok(CiState::Pending {
            run_id: if run_id.is_empty() {
                None
            } else {
                Some(run_id)
            },
        });
    }
    if conclusion == "success" {
        Ok(CiState::Passed {
            run_id,
            run_url: url,
            conclusion,
        })
    } else {
        Ok(CiState::Failed {
            run_id,
            run_url: url,
            conclusion,
            log_excerpt: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_gh_run_list_returns_pending_when_no_run_exists_yet() {
        let state = parse_gh_run_list(br#"[]"#).unwrap();
        assert_eq!(state, CiState::Pending { run_id: None });
    }

    #[test]
    fn parse_gh_run_list_maps_successful_completed_run() {
        let state = parse_gh_run_list(
            br#"[{"databaseId":42,"status":"completed","conclusion":"success","url":"https://example.test/run"}]"#,
        )
        .unwrap();
        assert_eq!(
            state,
            CiState::Passed {
                run_id: "42".into(),
                run_url: "https://example.test/run".into(),
                conclusion: "success".into()
            }
        );
    }

    #[test]
    fn parse_gh_run_list_maps_failed_completed_run() {
        let state = parse_gh_run_list(
            br#"[{"databaseId":99,"status":"completed","conclusion":"failure","url":"https://example.test/run"}]"#,
        )
        .unwrap();
        assert_eq!(
            state,
            CiState::Failed {
                run_id: "99".into(),
                run_url: "https://example.test/run".into(),
                conclusion: "failure".into(),
                log_excerpt: None
            }
        );
    }

    #[test]
    fn ci_tracker_args_include_explicit_repo_when_configured() {
        let tracker = SshCiTracker::with_repo(Some("example/repo".into()));
        let list_args = tracker.gh_run_list_args("abc123", "main");
        assert!(list_args
            .windows(2)
            .any(|pair| pair[0] == "--repo" && pair[1] == "example/repo"));
        let view_args = tracker.gh_run_view_args("42");
        assert!(view_args
            .windows(2)
            .any(|pair| pair[0] == "--repo" && pair[1] == "example/repo"));
    }
}

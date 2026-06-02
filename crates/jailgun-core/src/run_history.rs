use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::RunSnapshot;

/// Aggregated statistics about code changes in a completed run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunCodeStats {
    pub total_files_changed: u32,
    pub total_additions: u32,
    pub total_deletions: u32,
    pub total_test_count: u32,
}

/// Persistent summary of a completed run for historical tracking.
///
/// Written to `data/run-history/` at run completion and served by
/// `GET /api/history` so the dashboard can render progress-over-time
/// charts without requiring a database.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunHistoryEntry {
    pub run_id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub batch_tabs: u16,
    pub loop_count: u16,
    pub planned_tabs: u16,
    pub total_tabs: u16,
    pub tabs_passed: u16,
    pub tabs_failed: u16,
    pub tabs_pushed: u16,
    pub deploy_queue_final: String,
    pub denied_github_prompts: u32,
    pub allowed_info_prompts: u32,
    pub early_stops_succeeded: u16,
    pub early_stops_attempted: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_stats: Option<RunCodeStats>,
}

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("history I/O error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("history serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Derive a history entry from a completed run snapshot.
pub fn summarize_run(snapshot: &RunSnapshot) -> RunHistoryEntry {
    let mut tabs_passed: u16 = 0;
    let mut tabs_failed: u16 = 0;
    let mut tabs_pushed: u16 = 0;
    for tab in &snapshot.tabs {
        match tab.deploy_status.as_str() {
            "succeeded" | "done" | "dry-run-staged" | "succeeded-ci-skipped" => {
                tabs_passed = tabs_passed.saturating_add(1);
                tabs_pushed = tabs_pushed.saturating_add(1);
            }
            "failed-preserved" | "failed-hard" | "upload-sha-mismatch" | "timed-out" | "error" => {
                tabs_failed = tabs_failed.saturating_add(1);
            }
            _ => {}
        }
    }
    RunHistoryEntry {
        run_id: snapshot.run_id.clone(),
        started_at: snapshot.started_at.clone(),
        finished_at: snapshot.finished_at.clone(),
        status: snapshot.status.clone(),
        batch_tabs: snapshot.batch_tabs,
        loop_count: snapshot.loop_count,
        planned_tabs: snapshot.planned_tabs,
        total_tabs: snapshot.tabs.len().min(u16::MAX as usize) as u16,
        tabs_passed,
        tabs_failed,
        tabs_pushed,
        deploy_queue_final: serde_json::to_string(&snapshot.deploy_queue)
            .unwrap_or_else(|_| "unknown".into()),
        denied_github_prompts: snapshot.denied_github_prompts,
        allowed_info_prompts: snapshot.allowed_info_prompts,
        early_stops_succeeded: snapshot.early_stops_succeeded,
        early_stops_attempted: snapshot.early_stops_attempted,
        code_stats: None,
    }
}

/// Atomically write a run history entry to disk.
///
/// Uses the same tmp/rename pattern as the JMCP envelope writer to
/// prevent partial reads.
pub fn write_run_history(
    dir: impl AsRef<Path>,
    entry: &RunHistoryEntry,
) -> Result<PathBuf, HistoryError> {
    let dir = dir.as_ref();
    fs::create_dir_all(dir).map_err(|source| HistoryError::Io {
        path: dir.display().to_string(),
        source,
    })?;
    let tmp_dir = dir.join("tmp");
    fs::create_dir_all(&tmp_dir).map_err(|source| HistoryError::Io {
        path: tmp_dir.display().to_string(),
        source,
    })?;
    let filename = format!("run-summary-{}.json", entry.run_id);
    let tmp_path = tmp_dir.join(format!("{filename}.partial"));
    let final_path = dir.join(&filename);

    let bytes = serde_json::to_vec_pretty(entry)?;
    let mut file = fs::File::create(&tmp_path).map_err(|source| HistoryError::Io {
        path: tmp_path.display().to_string(),
        source,
    })?;
    file.write_all(&bytes).map_err(|source| HistoryError::Io {
        path: tmp_path.display().to_string(),
        source,
    })?;
    file.sync_all().map_err(|source| HistoryError::Io {
        path: tmp_path.display().to_string(),
        source,
    })?;
    drop(file);
    fs::rename(&tmp_path, &final_path).map_err(|source| HistoryError::Io {
        path: final_path.display().to_string(),
        source,
    })?;
    Ok(final_path)
}

/// Read all run history entries from disk, sorted by `started_at`
/// ascending.
pub fn read_run_history(dir: impl AsRef<Path>) -> Result<Vec<RunHistoryEntry>, HistoryError> {
    let dir = dir.as_ref();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    let read_dir = fs::read_dir(dir).map_err(|source| HistoryError::Io {
        path: dir.display().to_string(),
        source,
    })?;
    for entry in read_dir {
        let entry = entry.map_err(|source| HistoryError::Io {
            path: dir.display().to_string(),
            source,
        })?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|source| HistoryError::Io {
            path: path.display().to_string(),
            source,
        })?;
        match serde_json::from_str::<RunHistoryEntry>(&text) {
            Ok(entry) => entries.push(entry),
            Err(_) => continue,
        }
    }
    entries.sort_by(|a, b| a.started_at.cmp(&b.started_at));
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeployQueueState, TabSnapshot};

    fn sample_snapshot() -> RunSnapshot {
        RunSnapshot {
            run_id: "run-history-test".into(),
            started_at: "2026-01-01T00:00:00Z".into(),
            finished_at: Some("2026-01-01T01:00:00Z".into()),
            status: "finished".into(),
            batch_tabs: 7,
            loop_count: 2,
            planned_tabs: 21,
            deploy_queue: DeployQueueState::Done,
            denied_github_prompts: 3,
            allowed_info_prompts: 1,
            early_stops_succeeded: 2,
            early_stops_attempted: 4,
            tabs: vec![
                TabSnapshot {
                    tab_id: 1,
                    status: "deployed".into(),
                    page_url: "https://chatgpt.com/c/1".into(),
                    archive_sha256: Some("abc".into()),
                    download_latency_ms: Some(900),
                    deploy_status: "succeeded".into(),
                    prompt_policy_decision: None,
                    early_stop_outcome: None,
                    browser_profile: None,
                    browser_profile_dir: None,
                    browser_slot: None,
                    cdp_url: None,
                },
                TabSnapshot {
                    tab_id: 2,
                    status: "error".into(),
                    page_url: "https://chatgpt.com/c/2".into(),
                    archive_sha256: Some("def".into()),
                    download_latency_ms: Some(1100),
                    deploy_status: "failed-preserved".into(),
                    prompt_policy_decision: Some("deny".into()),
                    early_stop_outcome: Some("attempted".into()),
                    browser_profile: None,
                    browser_profile_dir: None,
                    browser_slot: None,
                    cdp_url: None,
                },
            ],
        }
    }

    #[test]
    fn summarize_run_counts_pass_and_fail() {
        let snapshot = sample_snapshot();
        let entry = summarize_run(&snapshot);
        assert_eq!(entry.run_id, "run-history-test");
        assert_eq!(entry.tabs_passed, 1);
        assert_eq!(entry.tabs_failed, 1);
        assert_eq!(entry.tabs_pushed, 1);
        assert_eq!(entry.total_tabs, 2);
        assert_eq!(entry.batch_tabs, 7);
        assert_eq!(entry.loop_count, 2);
        assert_eq!(entry.planned_tabs, 21);
        assert_eq!(entry.denied_github_prompts, 3);
    }

    #[test]
    fn serialization_round_trip() {
        let entry = summarize_run(&sample_snapshot());
        let json = serde_json::to_string_pretty(&entry).expect("serialize");
        let parsed: RunHistoryEntry = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(entry, parsed);
    }

    #[test]
    fn write_and_read_history() {
        let temp = tempfile::tempdir().expect("tempdir");
        let entry = summarize_run(&sample_snapshot());
        let path = write_run_history(temp.path(), &entry).expect("write");
        assert!(path.exists());
        let entries = read_run_history(temp.path()).expect("read");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], entry);
    }

    #[test]
    fn read_empty_directory_returns_empty_vec() {
        let temp = tempfile::tempdir().expect("tempdir");
        let entries = read_run_history(temp.path()).expect("read");
        assert!(entries.is_empty());
    }

    #[test]
    fn read_nonexistent_directory_returns_empty_vec() {
        let entries = read_run_history("/nonexistent/path/run-history").expect("read");
        assert!(entries.is_empty());
    }

    #[test]
    fn multiple_entries_sorted_by_started_at() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut entry_a = summarize_run(&sample_snapshot());
        entry_a.run_id = "run-b".into();
        entry_a.started_at = "2026-01-02T00:00:00Z".into();
        let mut entry_b = summarize_run(&sample_snapshot());
        entry_b.run_id = "run-a".into();
        entry_b.started_at = "2026-01-01T00:00:00Z".into();
        write_run_history(temp.path(), &entry_a).expect("write a");
        write_run_history(temp.path(), &entry_b).expect("write b");
        let entries = read_run_history(temp.path()).expect("read");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].run_id, "run-a");
        assert_eq!(entries[1].run_id, "run-b");
    }

    #[test]
    fn code_stats_optional_in_serialization() {
        let mut entry = summarize_run(&sample_snapshot());
        let json_without = serde_json::to_string(&entry).expect("serialize");
        assert!(!json_without.contains("code_stats"));

        entry.code_stats = Some(RunCodeStats {
            total_files_changed: 5,
            total_additions: 120,
            total_deletions: 30,
            total_test_count: 8,
        });
        let json_with = serde_json::to_string(&entry).expect("serialize");
        assert!(json_with.contains("total_additions"));
        let parsed: RunHistoryEntry = serde_json::from_str(&json_with).expect("deserialize");
        assert_eq!(parsed.code_stats.unwrap().total_additions, 120);
    }
}

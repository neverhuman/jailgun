//! Commit notice payload + the body-markdown formatter used by the JMCP
//! envelope. The Telegram path is gone; this module is now pure data + text.

use serde::{Deserialize, Serialize};

use crate::envelope::{NotifyCommitPayload, Payload};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitNotice {
    pub run_id: String,
    pub tab_id: Option<u16>,
    pub post_head: String,
    pub pre_head: Option<String>,
    pub files_changed: usize,
    pub additions: u64,
    pub deletions: u64,
    pub top_paths: Vec<String>,
    pub ci_state: Option<String>,
    pub remote_command_exit: Option<i32>,
}

const SUMMARY_EMOJI: &str = "✅";

pub fn build_commit_notice_message(notice: &CommitNotice) -> String {
    let tab = notice
        .tab_id
        .map(|tab_id| format!("tab {tab_id}"))
        .unwrap_or_else(|| "tab unknown".to_string());
    let mut lines = vec![
        format!("{SUMMARY_EMOJI} Jailgun commit succeeded"),
        format!("run {} ({tab})", notice.run_id),
        format!("head {}", notice.post_head),
    ];

    if let Some(pre_head) = &notice.pre_head {
        lines.push(format!("from {pre_head}"));
    }

    lines.push(format!(
        "{} files, +{}, -{}",
        notice.files_changed, notice.additions, notice.deletions
    ));

    if let Some(ci_state) = &notice.ci_state {
        lines.push(format!("ci {ci_state}"));
    }
    if let Some(exit) = notice.remote_command_exit {
        lines.push(format!("remote exit {exit}"));
    }
    if !notice.top_paths.is_empty() {
        lines.push("Files:".to_string());
        for path in notice.top_paths.iter().take(12) {
            lines.push(format!("- {path}"));
        }
        if notice.top_paths.len() > 12 {
            lines.push(format!("- … {} more", notice.top_paths.len() - 12));
        }
    }

    lines.join("\n")
}

/// Lift a `CommitNotice` into the structured JMCP payload variant. The body
/// is the same human-readable text the Telegram path used to send.
pub fn commit_notice_to_payload(notice: &CommitNotice) -> Payload {
    Payload::NotifyCommit(NotifyCommitPayload {
        title: "Jailgun commit succeeded".to_string(),
        summary_emoji: SUMMARY_EMOJI.to_string(),
        body_markdown: build_commit_notice_message(notice),
        run_id: notice.run_id.clone(),
        tab_id: notice.tab_id,
        post_head: notice.post_head.clone(),
        pre_head: notice.pre_head.clone(),
        files_changed: notice.files_changed,
        additions: notice.additions,
        deletions: notice.deletions,
        top_paths: notice.top_paths.clone(),
        ci_state: notice.ci_state.clone(),
        remote_command_exit: notice.remote_command_exit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_commit_notice_message() {
        let message = build_commit_notice_message(&CommitNotice {
            run_id: "run-1".into(),
            tab_id: Some(3),
            post_head: "abc1234".into(),
            pre_head: Some("def5678".into()),
            files_changed: 2,
            additions: 12,
            deletions: 4,
            top_paths: vec!["src/lib.rs".into(), "README.md".into()],
            ci_state: Some("passed".into()),
            remote_command_exit: Some(0),
        });

        assert!(message.contains("Jailgun commit succeeded"));
        assert!(message.contains("run run-1 (tab 3)"));
        assert!(message.contains("2 files, +12, -4"));
        assert!(message.contains("- README.md"));
    }

    #[test]
    fn payload_carries_top_paths_and_metrics() {
        let notice = CommitNotice {
            run_id: "run-1".into(),
            tab_id: None,
            post_head: "abcdef".into(),
            pre_head: None,
            files_changed: 1,
            additions: 5,
            deletions: 0,
            top_paths: vec!["src/main.rs".into()],
            ci_state: None,
            remote_command_exit: None,
        };
        let payload = commit_notice_to_payload(&notice);
        match payload {
            Payload::NotifyCommit(p) => {
                assert_eq!(p.run_id, "run-1");
                assert_eq!(p.summary_emoji, "✅");
                assert_eq!(p.top_paths, vec!["src/main.rs"]);
                assert!(p.body_markdown.contains("Jailgun commit succeeded"));
            }
            _ => panic!("expected NotifyCommit"),
        }
    }
}

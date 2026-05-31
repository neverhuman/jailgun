use std::{
    path::Path,
    process::{Command, Stdio},
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CommitError {
    #[error("git command failed: {0}")]
    Git(String),
    #[error("commit summary is missing")]
    MissingSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitSummary {
    pub short_hash: String,
    pub subject: String,
    pub files: Vec<String>,
}

pub fn collect_commit_summary(
    repo: impl AsRef<Path>,
    revision: &str,
) -> Result<CommitSummary, CommitError> {
    let repo = repo.as_ref();
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([
            "show",
            "--name-only",
            "--format=%h%n%s",
            "--no-renames",
            revision,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| CommitError::Git(error.to_string()))?;
    if !output.status.success() {
        return Err(CommitError::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    parse_git_show(&String::from_utf8_lossy(&output.stdout))
}

pub fn build_commit_message(summary: &CommitSummary) -> String {
    let mut lines = vec![
        "✅ Jailgun commit succeeded".to_string(),
        format!("{} {}", summary.short_hash, summary.subject),
    ];
    if summary.files.is_empty() {
        lines.push("Files: none reported".to_string());
    } else {
        lines.push("Files:".to_string());
        for file in summary.files.iter().take(12) {
            lines.push(format!("- {file}"));
        }
        if summary.files.len() > 12 {
            lines.push(format!("- … {} more", summary.files.len() - 12));
        }
    }
    lines.join("\n")
}

fn parse_git_show(text: &str) -> Result<CommitSummary, CommitError> {
    let mut lines = text.lines();
    let short_hash = lines
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .ok_or(CommitError::MissingSummary)?
        .to_string();
    let subject = lines
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .ok_or(CommitError::MissingSummary)?
        .to_string();
    let files = lines
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    Ok(CommitSummary {
        short_hash,
        subject,
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_short_commit_message_with_files() {
        let summary = CommitSummary {
            short_hash: "abc1234".into(),
            subject: "add notifier".into(),
            files: vec![
                "crates/jailgun-notify/src/lib.rs".into(),
                "README.md".into(),
            ],
        };
        let message = build_commit_message(&summary);
        assert!(message.contains("Jailgun commit succeeded"));
        assert!(message.contains("abc1234 add notifier"));
        assert!(message.contains("- README.md"));
    }

    #[test]
    fn parses_git_show_output() {
        let parsed =
            parse_git_show("abc1234\nsubject line\nsrc/lib.rs\nREADME.md\n").expect("parsed");
        assert_eq!(parsed.short_hash, "abc1234");
        assert_eq!(parsed.subject, "subject line");
        assert_eq!(parsed.files, vec!["src/lib.rs", "README.md"]);
    }
}

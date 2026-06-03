use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{send_telegram_message, TelegramConfig, TelegramError};

#[derive(Debug, Error)]
pub enum NotifyError {
    #[error(transparent)]
    Telegram(#[from] TelegramError),
    #[error("could not read Telegram chat id cache {path}: {source}")]
    ReadChatIdCache {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not write Telegram chat id cache {path}: {source}")]
    WriteChatIdCache {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

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

pub async fn send_commit_notice(
    token_path: &Path,
    chat_id_cache: &Path,
    notice: &CommitNotice,
) -> Result<(), NotifyError> {
    let mut config = TelegramConfig::from_token_file(token_path)?;
    if config.chat_id.is_none() {
        config.chat_id = read_chat_id_cache(chat_id_cache)?;
    }

    let chat_id = send_telegram_message(&config, &build_commit_notice_message(notice)).await?;
    write_chat_id_cache(chat_id_cache, &chat_id)?;
    Ok(())
}

pub fn build_commit_notice_message(notice: &CommitNotice) -> String {
    let tab = notice
        .tab_id
        .map(|tab_id| format!("tab {tab_id}"))
        .unwrap_or_else(|| "tab unknown".to_string());
    let mut lines = vec![
        "✅ Jailgun commit succeeded".to_string(),
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

pub fn read_chat_id_cache(path: &Path) -> Result<Option<String>, NotifyError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with('#'))
            .map(ToOwned::to_owned)),
        Err(source) if source.kind() == ErrorKind::NotFound => Ok(None),
        Err(source) => Err(NotifyError::ReadChatIdCache {
            path: path.to_path_buf(),
            source,
        }),
    }
}

pub fn write_chat_id_cache(path: &Path, chat_id: &str) -> Result<(), NotifyError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| NotifyError::WriteChatIdCache {
            path: path.to_path_buf(),
            source,
        })?;
    }
    fs::write(path, format!("{chat_id}\n")).map_err(|source| NotifyError::WriteChatIdCache {
        path: path.to_path_buf(),
        source,
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
    fn reads_and_writes_chat_id_cache() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("telegram").join("chat_id.cache");

        assert_eq!(read_chat_id_cache(&path).expect("missing cache"), None);
        write_chat_id_cache(&path, "-10042").expect("write cache");
        assert_eq!(
            read_chat_id_cache(&path).expect("read cache").as_deref(),
            Some("-10042")
        );
    }
}

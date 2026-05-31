//! Broadcast subscriber that fires terse Telegram notifications at the three
//! milestones the user cares about:
//!
//! 1. Job started on a tab (`EventKind::PromptSubmitted`)
//! 2. Tar acquired (`EventKind::DownloadReceipt`)
//! 3. Remote CI passed and the commit landed on `main`
//!    (`EventKind::DeployFinished` with `outcome=succeeded` and
//!    `ci_state=passed`)
//!
//! Plus a brief failure ping when a deploy ends with severity=Error so the
//! user knows something broke without watching the dashboard.

use std::path::{Path, PathBuf};

use jailgun_core::{EventKind, JailgunEvent, Severity};
use tokio::sync::broadcast::{self, error::RecvError};

use crate::notice::{read_chat_id_cache, write_chat_id_cache};
use crate::{send_telegram_message, NotifyError, TelegramConfig};

/// Spawn-and-forget loop. Reads events off the broadcast, formats Telegram
/// notices for the milestones we care about, and sends them. Logs and skips
/// transient Telegram failures so the subscriber never blocks the run.
pub async fn run_telegram_subscriber(
    mut rx: broadcast::Receiver<JailgunEvent>,
    token_path: PathBuf,
    chat_id_cache: PathBuf,
) {
    loop {
        match rx.recv().await {
            Ok(event) => {
                let Some(text) = format_event_notice(&event) else {
                    continue;
                };
                if let Err(error) = send_one(&token_path, &chat_id_cache, &text).await {
                    tracing::warn!(?error, "telegram notify failed");
                }
            }
            Err(RecvError::Lagged(dropped)) => {
                tracing::warn!(dropped, "telegram subscriber lagged");
            }
            Err(RecvError::Closed) => return,
        }
    }
}

async fn send_one(token_path: &Path, chat_id_cache: &Path, text: &str) -> Result<(), NotifyError> {
    let mut config = TelegramConfig::from_token_file(token_path)?;
    if config.chat_id.is_none() {
        config.chat_id = read_chat_id_cache(chat_id_cache)?;
    }
    let chat_id = send_telegram_message(&config, text).await?;
    write_chat_id_cache(chat_id_cache, &chat_id)?;
    Ok(())
}

/// Returns the short notification body for the events the user asked about,
/// or `None` for events we deliberately do not ping on.
pub fn format_event_notice(event: &JailgunEvent) -> Option<String> {
    let tab = event
        .tab_id
        .map(|t| format!("tab {t}"))
        .unwrap_or_else(|| "tab ?".into());
    match event.kind {
        EventKind::PromptSubmitted => Some(format!("▶ {} · {} · job started", event.run_id, tab)),
        EventKind::DownloadReceipt => {
            let sha = event
                .fields
                .get("sha256")
                .map(|s| s.chars().take(8).collect::<String>())
                .unwrap_or_else(|| "?".into());
            Some(format!(
                "📦 {} · {} · tar acquired · sha {}",
                event.run_id, tab, sha
            ))
        }
        EventKind::DeployFinished => format_deploy_finished(event, &tab),
        _ => None,
    }
}

fn format_deploy_finished(event: &JailgunEvent, tab: &str) -> Option<String> {
    let outcome = event.fields.get("outcome").map(String::as_str)?;
    let ci_state = event.fields.get("ci_state").map(String::as_str);

    if event.severity == Severity::Error {
        let reason = event
            .fields
            .get("failure_reason")
            .map(String::as_str)
            .unwrap_or("unknown");
        let preserved = event
            .fields
            .get("preserved_ref")
            .map(|s| format!(" · preserved {s}"))
            .unwrap_or_default();
        return Some(format!(
            "❌ {} · {} · deploy {outcome} · {reason}{preserved}",
            event.run_id, tab
        ));
    }

    if outcome != "succeeded" {
        return None;
    }

    let post_head = event
        .fields
        .get("post_head")
        .map(|s| s.chars().take(8).collect::<String>())
        .unwrap_or_else(|| "?".into());

    let files_line = match (
        event.fields.get("files_changed"),
        event.fields.get("additions"),
        event.fields.get("deletions"),
    ) {
        (Some(files), Some(adds), Some(dels)) => format!(" · {files} files +{adds} -{dels}"),
        _ => String::new(),
    };

    let ci_line = match ci_state {
        Some("passed") => " · CI passed, on main".to_string(),
        Some("skipped") => " · CI skipped (no commit)".to_string(),
        _ => String::new(),
    };

    let paths = event
        .fields
        .get("top_paths")
        .map(|raw| raw.clone())
        .unwrap_or_default();
    let paths_line = if paths.is_empty() {
        String::new()
    } else {
        format!("\nfiles: {paths}")
    };

    Some(format!(
        "✅ {} · {} · {post_head}{ci_line}{files_line}{paths_line}",
        event.run_id, tab
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jailgun_core::JailgunEvent;

    fn base(kind: EventKind, tab: u16) -> JailgunEvent {
        JailgunEvent::new("run-A".to_string(), kind, "x".to_string()).with_tab(tab)
    }

    #[test]
    fn prompt_submitted_emits_job_started_line() {
        let msg = format_event_notice(&base(EventKind::PromptSubmitted, 2)).expect("some");
        assert!(msg.contains("job started"));
        assert!(msg.contains("tab 2"));
        assert!(msg.contains("run-A"));
    }

    #[test]
    fn download_receipt_includes_short_sha() {
        let event =
            base(EventKind::DownloadReceipt, 1).with_field("sha256", "abcdef0123456789".repeat(4));
        let msg = format_event_notice(&event).expect("some");
        assert!(msg.contains("tar acquired"));
        assert!(msg.contains("sha abcdef01"));
    }

    #[test]
    fn deploy_finished_success_with_ci_passed_includes_files_and_paths() {
        let event = base(EventKind::DeployFinished, 1)
            .with_field("outcome", "succeeded")
            .with_field("ci_state", "passed")
            .with_field("post_head", "abc1234deadbeef00")
            .with_field("files_changed", "3")
            .with_field("additions", "20")
            .with_field("deletions", "5")
            .with_field("top_paths", "src/lib.rs,README.md,Cargo.toml");
        let msg = format_event_notice(&event).expect("some");
        assert!(msg.contains("CI passed, on main"));
        assert!(msg.contains("abc1234d"));
        assert!(msg.contains("3 files +20 -5"));
        assert!(msg.contains("files: src/lib.rs,README.md,Cargo.toml"));
    }

    #[test]
    fn deploy_finished_failure_emits_short_failure_line() {
        let event = base(EventKind::DeployFinished, 1)
            .with_severity(Severity::Error)
            .with_field("outcome", "failed-preserved")
            .with_field("failure_reason", "remote-command-failed")
            .with_field("preserved_ref", "jailgun-failed/run-A-tab-01");
        let msg = format_event_notice(&event).expect("some");
        assert!(msg.starts_with('❌'));
        assert!(msg.contains("remote-command-failed"));
        assert!(msg.contains("preserved jailgun-failed/run-A-tab-01"));
    }

    #[test]
    fn other_event_kinds_return_none() {
        let event = base(EventKind::TabOpened, 1);
        assert!(format_event_notice(&event).is_none());
    }

    #[test]
    fn deploy_finished_ci_skipped_does_not_get_files_block_when_absent() {
        let event = base(EventKind::DeployFinished, 1)
            .with_field("outcome", "succeeded")
            .with_field("ci_state", "skipped")
            .with_field("post_head", "aaaaaaaa");
        let msg = format_event_notice(&event).expect("some");
        assert!(msg.contains("CI skipped"));
        assert!(!msg.contains("files:"));
    }
}

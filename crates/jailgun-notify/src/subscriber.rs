//! Broadcast subscriber that turns jailgun events into JMCP envelopes and
//! drops them into the inbox. The bridge (e.g.
//! `~/code/jmcp/bin/jmcp-bridge.mjs`) takes it from there.

use std::collections::BTreeMap;
use std::path::PathBuf;

use jailgun_core::{EventKind, JailgunEvent, Severity};
use thiserror::Error;
use tokio::sync::broadcast::{self, error::RecvError};

use crate::envelope::{JmcpEnvelope, NotifyEventPayload, Payload, Routing, TaskRef};
use crate::writer::{JmcpInbox, JmcpInboxError};

#[derive(Debug, Error)]
pub enum JmcpSubscriberError {
    #[error(transparent)]
    Inbox(#[from] JmcpInboxError),
}

/// Spawn-and-forget loop. Reads events, builds envelopes for the milestones
/// we care about, writes them to the inbox. Logs and skips transient write
/// failures so the subscriber never blocks the run.
pub async fn run_jmcp_subscriber(mut rx: broadcast::Receiver<JailgunEvent>, inbox_dir: PathBuf) {
    let inbox = JmcpInbox::new(inbox_dir);
    loop {
        match rx.recv().await {
            Ok(event) => {
                let Some(payload) = event_to_payload(&event) else {
                    continue;
                };
                let envelope = JmcpEnvelope::new(
                    payload,
                    TaskRef::for_run(&event.run_id, event.tab_id),
                    Routing::notify_user(),
                );
                match inbox.write_envelope(&envelope).await {
                    Ok(path) => {
                        tracing::info!(
                            run_id = %event.run_id,
                            tab_id = ?event.tab_id,
                            kind = ?event.kind,
                            envelope = ?path,
                            "jmcp envelope written"
                        );
                    }
                    Err(error) => {
                        tracing::warn!(?error, "jmcp envelope write failed");
                    }
                }
            }
            Err(RecvError::Lagged(dropped)) => {
                tracing::warn!(dropped, "jmcp subscriber lagged");
            }
            Err(RecvError::Closed) => return,
        }
    }
}

/// Build a JMCP payload for the milestones the user cares about, or `None`
/// for events we deliberately skip.
pub fn event_to_payload(event: &JailgunEvent) -> Option<Payload> {
    let body = format_event_notice(event)?;
    let (title, emoji, severity_label) = match event.kind {
        EventKind::PromptSubmitted => ("Job started", "▶", "info"),
        EventKind::DownloadReceipt => ("Tar acquired", "📦", "info"),
        EventKind::DeployFinished if event.severity == Severity::Error => {
            ("Deploy failed", "❌", "error")
        }
        EventKind::DeployFinished => ("Deploy succeeded", "✅", "info"),
        _ => return None,
    };
    let metrics = collect_metrics(event);
    Some(Payload::NotifyEvent(NotifyEventPayload {
        title: title.to_string(),
        summary_emoji: emoji.to_string(),
        body_markdown: body,
        run_id: event.run_id.clone(),
        tab_id: event.tab_id,
        event_kind: format!("{:?}", event.kind),
        event_severity: severity_label.to_string(),
        metrics,
    }))
}

fn collect_metrics(event: &JailgunEvent) -> BTreeMap<String, String> {
    let mut metrics = BTreeMap::new();
    for key in [
        "outcome",
        "ci_state",
        "exit_code",
        "files_changed",
        "additions",
        "deletions",
        "sha256",
        "post_head",
        "phase",
        "early_stops_succeeded",
        "early_stops_attempted",
    ] {
        if let Some(value) = event.fields.get(key) {
            metrics.insert(key.to_string(), value.clone());
        }
    }
    metrics
}

/// Human-readable notification body — same format the Telegram path used to
/// send, kept verbatim so the bridge has a ready-to-render markdown string.
pub fn format_event_notice(event: &JailgunEvent) -> Option<String> {
    let tab = match event.tab_id {
        Some(t) => format!("tab {t}"),
        None => "tab ?".to_string(),
    };
    match event.kind {
        EventKind::PromptSubmitted => Some(format!("▶ {} · {} · job started", event.run_id, tab)),
        EventKind::DownloadReceipt => {
            let sha = match event.fields.get("sha256") {
                Some(value) => value.chars().take(8).collect::<String>(),
                None => "?".to_string(),
            };
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
        let reason = match event.fields.get("failure_reason") {
            Some(value) => value.as_str(),
            None => "unknown",
        };
        let preserved = match event.fields.get("preserved_ref") {
            Some(value) => format!(" · preserved {value}"),
            None => String::new(),
        };
        let exit = match event.fields.get("exit_code") {
            Some(value) => format!(" · exit {value}"),
            None => String::new(),
        };
        return Some(format!(
            "❌ {} · {} · deploy {outcome} · {reason}{exit}{preserved}",
            event.run_id, tab
        ));
    }

    let success_line = match (outcome, ci_state) {
        ("succeeded", Some("passed")) => " · CI passed, on main",
        ("succeeded-ci-skipped", _) => " · local CI/push complete",
        _ => return None,
    };

    let post_head = match event.fields.get("post_head") {
        Some(value) => value.chars().take(8).collect::<String>(),
        None => "?".to_string(),
    };

    let files_line = match (
        event.fields.get("files_changed"),
        event.fields.get("additions"),
        event.fields.get("deletions"),
    ) {
        (Some(files), Some(adds), Some(dels)) => format!(" · {files} files +{adds} -{dels}"),
        _ => String::new(),
    };

    let paths_line = match event.fields.get("top_paths") {
        Some(paths) if !paths.is_empty() => format!("\nfiles: {paths}"),
        _ => String::new(),
    };

    Some(format!(
        "✅ {} · {} · {post_head}{success_line}{files_line}{paths_line}",
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
            .with_field("exit_code", "127")
            .with_field("preserved_ref", "jailgun-failed/run-A-tab-01");
        let msg = format_event_notice(&event).expect("some");
        assert!(msg.starts_with('❌'));
        assert!(msg.contains("remote-command-failed"));
        assert!(msg.contains("exit 127"));
        assert!(msg.contains("preserved jailgun-failed/run-A-tab-01"));
    }

    #[test]
    fn other_event_kinds_return_none() {
        let event = base(EventKind::TabOpened, 1);
        assert!(format_event_notice(&event).is_none());
    }

    #[test]
    fn deploy_finished_ci_skipped_success_emits_local_ci_push_complete() {
        let event = base(EventKind::DeployFinished, 1)
            .with_field("outcome", "succeeded-ci-skipped")
            .with_field("ci_state", "skipped")
            .with_field("post_head", "aaaaaaaa");
        let msg = format_event_notice(&event).expect("some");
        assert!(msg.contains("local CI/push complete"));
        assert!(!msg.contains("files:"));
    }

    #[test]
    fn deploy_finished_plain_success_without_ci_passed_is_not_notified() {
        let event = base(EventKind::DeployFinished, 1)
            .with_field("outcome", "succeeded")
            .with_field("ci_state", "skipped")
            .with_field("post_head", "aaaaaaaa");
        assert!(format_event_notice(&event).is_none());
    }

    #[test]
    fn event_to_payload_wraps_success_with_metrics() {
        let event = base(EventKind::DeployFinished, 4)
            .with_field("outcome", "succeeded")
            .with_field("ci_state", "passed")
            .with_field("post_head", "abc1234d")
            .with_field("files_changed", "3")
            .with_field("additions", "10")
            .with_field("deletions", "2");
        let payload = event_to_payload(&event).expect("some");
        match payload {
            Payload::NotifyEvent(p) => {
                assert_eq!(p.summary_emoji, "✅");
                assert_eq!(p.event_severity, "info");
                assert_eq!(
                    p.metrics.get("ci_state").map(String::as_str),
                    Some("passed")
                );
                assert_eq!(
                    p.metrics.get("files_changed").map(String::as_str),
                    Some("3")
                );
            }
            _ => panic!("expected NotifyEvent"),
        }
    }
}

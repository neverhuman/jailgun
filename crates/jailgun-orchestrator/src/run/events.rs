//! Mapping from `BridgeEvent` to `JailgunEvent` for the broadcast bus.

use jailgun_core::{EventKind, JailgunEvent, Severity};

use crate::bridge::BridgeEvent;

pub fn map_bridge_event(
    run_id: &str,
    tab_id: Option<u16>,
    event: &BridgeEvent,
) -> Option<JailgunEvent> {
    let base = |kind: EventKind, message: &str| {
        let mut j = JailgunEvent::new(run_id.to_string(), kind, message.to_string());
        if let Some(t) = tab_id {
            j = j.with_tab(t);
        }
        j
    };
    let event = match event {
        BridgeEvent::BridgeReady(_) => return None,
        BridgeEvent::TabOpened(payload) => base(EventKind::TabOpened, "tab opened")
            .with_field("page_url", payload.page_url.clone()),
        BridgeEvent::ArchiveUploaded(payload) => base(EventKind::TabOpened, "archive uploaded")
            .with_field("sha256", payload.sha256.clone())
            .with_field("size_bytes", payload.size_bytes.to_string())
            .with_field("commit", payload.commit.clone())
            .with_field("archive_filename", payload.archive_filename.clone()),
        BridgeEvent::PromptSubmitted(payload) => {
            base(EventKind::PromptSubmitted, "prompt submitted")
                .with_field("char_count", payload.char_count.to_string())
        }
        BridgeEvent::TabProgress(_) => return None,
        BridgeEvent::TarDiscovered(_) => base(EventKind::TarDiscovered, "tar link discovered"),
        BridgeEvent::DownloadStarted(payload) => base(EventKind::TarDiscovered, "download started")
            .with_field("remote_url", payload.remote_url.clone())
            .with_field("target_path", payload.target_path.clone()),
        BridgeEvent::DownloadComplete(payload) => {
            base(EventKind::DownloadReceipt, "download complete")
                .with_field("sha256", payload.sha256.clone())
                .with_field("size_bytes", payload.size_bytes.to_string())
                .with_field("local_path", payload.local_path.clone())
                .with_field("receipt_path", payload.receipt_path.clone())
        }
        BridgeEvent::ToolPromptDetected(_) => return None,
        BridgeEvent::PromptPolicyApplied(payload) => {
            base(EventKind::PromptPolicy, "policy applied")
                .with_field("signature", payload.signature.clone())
                .with_field("decision", payload.decision.clone())
                .with_field(
                    "clicked",
                    if payload.clicked {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    },
                )
        }
        BridgeEvent::GenerationStopped(_) => return None,
        BridgeEvent::TabClosed(_) => return None,
        BridgeEvent::BridgeLog(_) => return None,
        BridgeEvent::Pong => return None,
        BridgeEvent::BridgeShuttingDown(_) => return None,
        BridgeEvent::Error(payload) => base(EventKind::Error, &payload.message)
            .with_severity(Severity::Error)
            .with_field("kind", payload.kind.clone())
            .with_field(
                "recoverable",
                if payload.recoverable {
                    "true".to_string()
                } else {
                    "false".to_string()
                },
            ),
    };
    Some(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::{ArchiveUploadedPayload, DownloadCompletePayload};

    #[test]
    fn maps_archive_uploaded() {
        let event = BridgeEvent::ArchiveUploaded(ArchiveUploadedPayload {
            sha256: "a".repeat(64),
            size_bytes: 4096,
            commit: "abc".into(),
            archive_filename: "source.tar.gz".into(),
            deleted_temp: true,
        });
        let mapped = map_bridge_event("run-1", Some(2), &event).expect("mapped");
        assert_eq!(mapped.run_id, "run-1");
        assert_eq!(mapped.tab_id, Some(2));
        assert_eq!(
            mapped.fields.get("archive_filename").map(String::as_str),
            Some("source.tar.gz")
        );
    }

    #[test]
    fn maps_download_complete_to_download_receipt_kind() {
        let event = BridgeEvent::DownloadComplete(DownloadCompletePayload {
            sha256: "b".repeat(64),
            size_bytes: 100,
            local_path: "/tmp/x.tar.gz".into(),
            receipt_path: "/tmp/r/x.tar.gz".into(),
            original_name: "x.tar.gz".into(),
            local_name: "x.tar.gz".into(),
            download_url: None,
            started_at: "2026-05-31T12:00:00Z".into(),
            finished_at: "2026-05-31T12:00:05Z".into(),
        });
        let mapped = map_bridge_event("run-1", Some(1), &event).expect("mapped");
        assert!(matches!(mapped.kind, EventKind::DownloadReceipt));
    }

    #[test]
    fn skips_noisy_tab_progress_events() {
        let payload = crate::bridge::TabProgressPayload {
            kind: crate::bridge::TabProgressKind::CompletionCheck,
            phase: "active".into(),
            busy_reason: None,
            has_active_stop: true,
            has_final_actions: false,
            last_text_length: 0,
            page_url: "https://example.invalid/".into(),
        };
        let event = BridgeEvent::TabProgress(payload);
        assert!(map_bridge_event("run-1", Some(1), &event).is_none());
    }
}

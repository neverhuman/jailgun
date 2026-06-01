//! `BridgeEvent` enum and per-variant payload structs.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::protocol::ProtocolError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeReadyPayload {
    pub node_version: String,
    pub playwright_version: String,
    pub browser: String,
    pub browser_version: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TabOpenedPayload {
    pub page_url: String,
    pub page_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchiveUploadedPayload {
    pub sha256: String,
    pub size_bytes: u64,
    pub commit: String,
    pub archive_filename: String,
    pub deleted_temp: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptSubmittedPayload {
    pub char_count: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TabProgressKind {
    CompletionCheck,
    Telemetry,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TabProgressPayload {
    pub kind: TabProgressKind,
    pub phase: String,
    #[serde(default)]
    pub busy_reason: Option<String>,
    pub has_active_stop: bool,
    pub has_final_actions: bool,
    pub last_text_length: u32,
    pub page_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TarDiscoveredPayload {
    pub candidates: serde_json::Value,
    #[serde(default)]
    pub selected_index: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DownloadStartedPayload {
    pub candidate_index: u32,
    pub remote_url: String,
    pub target_path: String,
    pub started_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DownloadCompletePayload {
    pub sha256: String,
    pub size_bytes: u64,
    pub local_path: String,
    pub receipt_path: String,
    pub original_name: String,
    pub local_name: String,
    #[serde(default)]
    pub download_url: Option<String>,
    #[serde(default)]
    pub entry_count: Option<u64>,
    #[serde(default)]
    pub download_latency_ms: Option<u64>,
    pub started_at: String,
    pub finished_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolPromptDetectedPayload {
    pub candidate: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptPolicyAppliedPayload {
    pub signature: String,
    pub decision: String,
    pub clicked: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RateLimitDetectedPayload {
    pub dismissed: bool,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenerationStoppedPayload {
    pub method: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TabClosedPayload {
    pub page_url: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeLogPayload {
    pub level: String,
    pub phase: String,
    pub message: String,
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeShuttingDownPayload {
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorPayload {
    pub kind: String,
    pub message: String,
    #[serde(default)]
    pub recoverable: bool,
    #[serde(default)]
    pub stack: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeEvent {
    BridgeReady(BridgeReadyPayload),
    TabOpened(TabOpenedPayload),
    ArchiveUploaded(ArchiveUploadedPayload),
    PromptSubmitted(PromptSubmittedPayload),
    TabProgress(TabProgressPayload),
    TarDiscovered(TarDiscoveredPayload),
    DownloadStarted(DownloadStartedPayload),
    DownloadComplete(DownloadCompletePayload),
    ToolPromptDetected(ToolPromptDetectedPayload),
    PromptPolicyApplied(PromptPolicyAppliedPayload),
    RateLimitDetected(RateLimitDetectedPayload),
    GenerationStopped(GenerationStoppedPayload),
    TabClosed(TabClosedPayload),
    BridgeLog(BridgeLogPayload),
    Pong,
    BridgeShuttingDown(BridgeShuttingDownPayload),
    Error(ErrorPayload),
}

impl BridgeEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            BridgeEvent::BridgeReady(_) => "bridge-ready",
            BridgeEvent::TabOpened(_) => "tab-opened",
            BridgeEvent::ArchiveUploaded(_) => "archive-uploaded",
            BridgeEvent::PromptSubmitted(_) => "prompt-submitted",
            BridgeEvent::TabProgress(_) => "tab-progress",
            BridgeEvent::TarDiscovered(_) => "tar-discovered",
            BridgeEvent::DownloadStarted(_) => "download-started",
            BridgeEvent::DownloadComplete(_) => "download-complete",
            BridgeEvent::ToolPromptDetected(_) => "tool-prompt-detected",
            BridgeEvent::PromptPolicyApplied(_) => "prompt-policy-applied",
            BridgeEvent::RateLimitDetected(_) => "rate-limit-detected",
            BridgeEvent::GenerationStopped(_) => "generation-stopped",
            BridgeEvent::TabClosed(_) => "tab-closed",
            BridgeEvent::BridgeLog(_) => "bridge-log",
            BridgeEvent::Pong => "pong",
            BridgeEvent::BridgeShuttingDown(_) => "bridge-shutting-down",
            BridgeEvent::Error(_) => "error",
        }
    }

    pub fn payload(&self) -> serde_json::Value {
        match self {
            BridgeEvent::BridgeReady(p) => super::protocol::to_value(p),
            BridgeEvent::TabOpened(p) => super::protocol::to_value(p),
            BridgeEvent::ArchiveUploaded(p) => super::protocol::to_value(p),
            BridgeEvent::PromptSubmitted(p) => super::protocol::to_value(p),
            BridgeEvent::TabProgress(p) => super::protocol::to_value(p),
            BridgeEvent::TarDiscovered(p) => super::protocol::to_value(p),
            BridgeEvent::DownloadStarted(p) => super::protocol::to_value(p),
            BridgeEvent::DownloadComplete(p) => super::protocol::to_value(p),
            BridgeEvent::ToolPromptDetected(p) => super::protocol::to_value(p),
            BridgeEvent::PromptPolicyApplied(p) => super::protocol::to_value(p),
            BridgeEvent::RateLimitDetected(p) => super::protocol::to_value(p),
            BridgeEvent::GenerationStopped(p) => super::protocol::to_value(p),
            BridgeEvent::TabClosed(p) => super::protocol::to_value(p),
            BridgeEvent::BridgeLog(p) => super::protocol::to_value(p),
            BridgeEvent::Pong => serde_json::json!({}),
            BridgeEvent::BridgeShuttingDown(p) => super::protocol::to_value(p),
            BridgeEvent::Error(p) => super::protocol::to_value(p),
        }
    }

    pub fn decode(kind: &str, payload: serde_json::Value) -> Result<BridgeEvent, ProtocolError> {
        let event = match kind {
            "bridge-ready" => BridgeEvent::BridgeReady(serde_json::from_value(payload)?),
            "tab-opened" => BridgeEvent::TabOpened(serde_json::from_value(payload)?),
            "archive-uploaded" => BridgeEvent::ArchiveUploaded(serde_json::from_value(payload)?),
            "prompt-submitted" => BridgeEvent::PromptSubmitted(serde_json::from_value(payload)?),
            "tab-progress" => BridgeEvent::TabProgress(serde_json::from_value(payload)?),
            "tar-discovered" => BridgeEvent::TarDiscovered(serde_json::from_value(payload)?),
            "download-started" => BridgeEvent::DownloadStarted(serde_json::from_value(payload)?),
            "download-complete" => BridgeEvent::DownloadComplete(serde_json::from_value(payload)?),
            "tool-prompt-detected" => {
                BridgeEvent::ToolPromptDetected(serde_json::from_value(payload)?)
            }
            "prompt-policy-applied" => {
                BridgeEvent::PromptPolicyApplied(serde_json::from_value(payload)?)
            }
            "rate-limit-detected" => {
                BridgeEvent::RateLimitDetected(serde_json::from_value(payload)?)
            }
            "generation-stopped" => {
                BridgeEvent::GenerationStopped(serde_json::from_value(payload)?)
            }
            "tab-closed" => BridgeEvent::TabClosed(serde_json::from_value(payload)?),
            "bridge-log" => BridgeEvent::BridgeLog(serde_json::from_value(payload)?),
            "pong" => BridgeEvent::Pong,
            "bridge-shutting-down" => {
                BridgeEvent::BridgeShuttingDown(serde_json::from_value(payload)?)
            }
            "error" => BridgeEvent::Error(serde_json::from_value(payload)?),
            other => return Err(ProtocolError::UnknownEvent(other.to_string())),
        };
        Ok(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::protocol::{decode_envelope, encode_envelope, envelope_for_event};

    #[test]
    fn roundtrip_download_complete_event() {
        let payload = DownloadCompletePayload {
            sha256: "a".repeat(64),
            size_bytes: 12345,
            local_path: "/tmp/x.tar.gz".into(),
            receipt_path: "/tmp/r/x.tar.gz".into(),
            original_name: "patch.tar.gz".into(),
            local_name: "patch.tar.gz".into(),
            download_url: Some("blob:https://chatgpt.com/x".into()),
            entry_count: Some(12),
            download_latency_ms: Some(8_000),
            started_at: "2026-05-31T12:00:00Z".into(),
            finished_at: "2026-05-31T12:00:08Z".into(),
        };
        let event = BridgeEvent::DownloadComplete(payload.clone());
        let envelope = envelope_for_event(&event, "run-test", "2026-05-31T12:00:00Z", Some(2));
        let line = encode_envelope(&envelope).expect("encode");
        let decoded = decode_envelope(line.trim_end()).expect("decode");
        let typed = BridgeEvent::decode(&decoded.kind, decoded.payload).expect("typed");
        match typed {
            BridgeEvent::DownloadComplete(got) => assert_eq!(got, payload),
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn unknown_event_kind_returns_protocol_error() {
        let err = BridgeEvent::decode("ghost-event", serde_json::json!({})).expect_err("unknown");
        assert!(matches!(err, ProtocolError::UnknownEvent(_)));
    }
}

//! NDJSON wire protocol between the Rust orchestrator and the Node
//! chrome-bridge child process.
//!
//! Each line on stdin or stdout is a JSON object that decodes to
//! `Envelope<serde_json::Value>`. The `kind` field is the discriminator; the
//! `payload` field is the variant-specific body. Use [`BridgeCommand::decode`]
//! and [`BridgeEvent::decode`] to lift a raw envelope into a typed value.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PROTOCOL_VERSION: u8 = 1;
pub const MAX_LINE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("line exceeded maximum size of {max} bytes (got {got})")]
    LineTooLong { got: usize, max: usize },
    #[error("could not decode envelope: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("unsupported protocol version {got} (this build speaks v{expected})")]
    UnsupportedVersion { got: u8, expected: u8 },
    #[error("unknown bridge command kind {0:?}")]
    UnknownCommand(String),
    #[error("unknown bridge event kind {0:?}")]
    UnknownEvent(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Envelope<P> {
    pub v: u8,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<u16>,
    pub ts: String,
    pub payload: P,
}

impl Envelope<serde_json::Value> {
    pub fn new(
        kind: impl Into<String>,
        run_id: impl Into<String>,
        ts: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            kind: kind.into(),
            id: None,
            correlation_id: None,
            run_id: run_id.into(),
            tab_id: None,
            ts: ts.into(),
            payload,
        }
    }

    pub fn with_tab(mut self, tab_id: u16) -> Self {
        self.tab_id = Some(tab_id);
        self
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HelloPayload {
    pub orchestrator_version: String,
    pub protocol_version: u8,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenTabPayload {
    pub chat_url: String,
    pub model: String,
    pub profile_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UploadArchivePayload {
    pub repo_url: String,
    #[serde(default = "default_ref_name")]
    pub ref_name: String,
    pub prefix: String,
    pub archive_filename: String,
    #[serde(default)]
    pub tmp_parent: Option<String>,
    #[serde(default = "default_delete_after_upload")]
    pub delete_after_upload: bool,
    #[serde(default)]
    pub confirm_selectors: Vec<String>,
    #[serde(default = "default_upload_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_ref_name() -> String {
    "HEAD".to_string()
}

fn default_delete_after_upload() -> bool {
    true
}

fn default_upload_timeout_ms() -> u64 {
    45_000
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubmitPromptPayload {
    pub prompt: String,
    pub submit_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MonitorTabPayload {
    pub completion_check_ms: u64,
    pub telemetry_tick_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloseTabPayload {
    #[serde(default)]
    pub run_before_unload: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApproveOrDenyPayload {
    pub signature: String,
    pub decision: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShutdownPayload {
    pub drain_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeCommand {
    Hello(HelloPayload),
    OpenTab(OpenTabPayload),
    UploadArchive(UploadArchivePayload),
    SubmitPrompt(SubmitPromptPayload),
    MonitorTab(MonitorTabPayload),
    StopGeneration,
    CloseTab(CloseTabPayload),
    ApproveOrDeny(ApproveOrDenyPayload),
    Shutdown(ShutdownPayload),
    Ping,
}

impl BridgeCommand {
    pub fn kind(&self) -> &'static str {
        match self {
            BridgeCommand::Hello(_) => "hello",
            BridgeCommand::OpenTab(_) => "open-tab",
            BridgeCommand::UploadArchive(_) => "upload-archive",
            BridgeCommand::SubmitPrompt(_) => "submit-prompt",
            BridgeCommand::MonitorTab(_) => "monitor-tab",
            BridgeCommand::StopGeneration => "stop-generation",
            BridgeCommand::CloseTab(_) => "close-tab",
            BridgeCommand::ApproveOrDeny(_) => "approve-or-deny",
            BridgeCommand::Shutdown(_) => "shutdown",
            BridgeCommand::Ping => "ping",
        }
    }

    pub fn payload(&self) -> serde_json::Value {
        match self {
            BridgeCommand::Hello(p) => serde_json::to_value(p).unwrap_or(serde_json::Value::Null),
            BridgeCommand::OpenTab(p) => serde_json::to_value(p).unwrap_or(serde_json::Value::Null),
            BridgeCommand::UploadArchive(p) => {
                serde_json::to_value(p).unwrap_or(serde_json::Value::Null)
            }
            BridgeCommand::SubmitPrompt(p) => {
                serde_json::to_value(p).unwrap_or(serde_json::Value::Null)
            }
            BridgeCommand::MonitorTab(p) => {
                serde_json::to_value(p).unwrap_or(serde_json::Value::Null)
            }
            BridgeCommand::StopGeneration => serde_json::json!({}),
            BridgeCommand::CloseTab(p) => {
                serde_json::to_value(p).unwrap_or(serde_json::Value::Null)
            }
            BridgeCommand::ApproveOrDeny(p) => {
                serde_json::to_value(p).unwrap_or(serde_json::Value::Null)
            }
            BridgeCommand::Shutdown(p) => {
                serde_json::to_value(p).unwrap_or(serde_json::Value::Null)
            }
            BridgeCommand::Ping => serde_json::json!({}),
        }
    }

    pub fn decode(kind: &str, payload: serde_json::Value) -> Result<BridgeCommand, ProtocolError> {
        let cmd = match kind {
            "hello" => BridgeCommand::Hello(serde_json::from_value(payload)?),
            "open-tab" => BridgeCommand::OpenTab(serde_json::from_value(payload)?),
            "upload-archive" => BridgeCommand::UploadArchive(serde_json::from_value(payload)?),
            "submit-prompt" => BridgeCommand::SubmitPrompt(serde_json::from_value(payload)?),
            "monitor-tab" => BridgeCommand::MonitorTab(serde_json::from_value(payload)?),
            "stop-generation" => BridgeCommand::StopGeneration,
            "close-tab" => BridgeCommand::CloseTab(serde_json::from_value(payload)?),
            "approve-or-deny" => BridgeCommand::ApproveOrDeny(serde_json::from_value(payload)?),
            "shutdown" => BridgeCommand::Shutdown(serde_json::from_value(payload)?),
            "ping" => BridgeCommand::Ping,
            other => return Err(ProtocolError::UnknownCommand(other.to_string())),
        };
        Ok(cmd)
    }
}

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
pub struct GenerationStoppedPayload {
    pub method: String,
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
            BridgeEvent::BridgeReady(p) => to_value(p),
            BridgeEvent::TabOpened(p) => to_value(p),
            BridgeEvent::ArchiveUploaded(p) => to_value(p),
            BridgeEvent::PromptSubmitted(p) => to_value(p),
            BridgeEvent::TabProgress(p) => to_value(p),
            BridgeEvent::TarDiscovered(p) => to_value(p),
            BridgeEvent::DownloadStarted(p) => to_value(p),
            BridgeEvent::DownloadComplete(p) => to_value(p),
            BridgeEvent::ToolPromptDetected(p) => to_value(p),
            BridgeEvent::PromptPolicyApplied(p) => to_value(p),
            BridgeEvent::GenerationStopped(p) => to_value(p),
            BridgeEvent::TabClosed(p) => to_value(p),
            BridgeEvent::BridgeLog(p) => to_value(p),
            BridgeEvent::Pong => serde_json::json!({}),
            BridgeEvent::BridgeShuttingDown(p) => to_value(p),
            BridgeEvent::Error(p) => to_value(p),
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

fn to_value<P: Serialize>(payload: &P) -> serde_json::Value {
    serde_json::to_value(payload).unwrap_or(serde_json::Value::Null)
}

pub fn encode_envelope(envelope: &Envelope<serde_json::Value>) -> Result<String, ProtocolError> {
    let mut text = serde_json::to_string(envelope)?;
    if text.len() > MAX_LINE_BYTES {
        return Err(ProtocolError::LineTooLong {
            got: text.len(),
            max: MAX_LINE_BYTES,
        });
    }
    text.push('\n');
    Ok(text)
}

pub fn decode_envelope(line: &str) -> Result<Envelope<serde_json::Value>, ProtocolError> {
    if line.len() > MAX_LINE_BYTES {
        return Err(ProtocolError::LineTooLong {
            got: line.len(),
            max: MAX_LINE_BYTES,
        });
    }
    let envelope: Envelope<serde_json::Value> = serde_json::from_str(line)?;
    if envelope.v != PROTOCOL_VERSION {
        return Err(ProtocolError::UnsupportedVersion {
            got: envelope.v,
            expected: PROTOCOL_VERSION,
        });
    }
    Ok(envelope)
}

pub fn envelope_for_command(
    command: &BridgeCommand,
    run_id: impl Into<String>,
    ts: impl Into<String>,
    tab_id: Option<u16>,
) -> Envelope<serde_json::Value> {
    let mut envelope = Envelope::new(command.kind(), run_id, ts, command.payload());
    envelope.tab_id = tab_id;
    envelope
}

pub fn envelope_for_event(
    event: &BridgeEvent,
    run_id: impl Into<String>,
    ts: impl Into<String>,
    tab_id: Option<u16>,
) -> Envelope<serde_json::Value> {
    let mut envelope = Envelope::new(event.kind(), run_id, ts, event.payload());
    envelope.tab_id = tab_id;
    envelope
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_open_tab_command() {
        let cmd = BridgeCommand::OpenTab(OpenTabPayload {
            chat_url: "https://chatgpt.com/".into(),
            model: "pro-extended".into(),
            profile_dir: "/tmp/profile".into(),
        });
        let envelope = envelope_for_command(&cmd, "run-test", "2026-05-31T12:00:00Z", Some(2));
        let line = encode_envelope(&envelope).expect("encode");
        assert!(line.ends_with('\n'));
        let decoded = decode_envelope(line.trim_end()).expect("decode");
        let typed = BridgeCommand::decode(&decoded.kind, decoded.payload).expect("typed");
        assert_eq!(typed, cmd);
    }

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
    fn rejects_wrong_version() {
        let line = r#"{"v":99,"type":"ping","run_id":"r","ts":"t","payload":{}}"#;
        let err = decode_envelope(line).expect_err("wrong version should be rejected");
        assert!(matches!(
            err,
            ProtocolError::UnsupportedVersion {
                got: 99,
                expected: PROTOCOL_VERSION
            }
        ));
    }

    #[test]
    fn rejects_oversized_line() {
        let huge = "a".repeat(MAX_LINE_BYTES + 1);
        let err = decode_envelope(&huge).expect_err("oversize");
        assert!(matches!(err, ProtocolError::LineTooLong { .. }));
    }

    #[test]
    fn unknown_command_kind_returns_protocol_error() {
        let err =
            BridgeCommand::decode("does-not-exist", serde_json::json!({})).expect_err("unknown");
        assert!(matches!(err, ProtocolError::UnknownCommand(_)));
    }

    #[test]
    fn unknown_event_kind_returns_protocol_error() {
        let err = BridgeEvent::decode("ghost-event", serde_json::json!({})).expect_err("unknown");
        assert!(matches!(err, ProtocolError::UnknownEvent(_)));
    }
}

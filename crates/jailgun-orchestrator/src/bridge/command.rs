//! `BridgeCommand` enum and per-variant payload structs.

use serde::{Deserialize, Serialize};

use super::protocol::ProtocolError;

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
    #[serde(default)]
    pub fresh_source_clone: bool,
    #[serde(default = "default_delete_after_upload")]
    pub delete_after_upload: bool,
    #[serde(default)]
    pub confirm_selectors: Vec<String>,
    #[serde(default = "default_upload_timeout_ms")]
    pub timeout_ms: u64,
    /// When set, the bridge fills this prompt into the composer while the
    /// upload is still processing and clicks send the instant the button
    /// becomes enabled.  This eliminates the round-trip delay between the
    /// `upload-archive` and `submit-prompt` commands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Timeout for the prompt submission readiness poll (only used when
    /// `prompt` is set).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submit_timeout_ms: Option<u64>,
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
            BridgeCommand::Hello(p) => super::protocol::to_value(p),
            BridgeCommand::OpenTab(p) => super::protocol::to_value(p),
            BridgeCommand::UploadArchive(p) => super::protocol::to_value(p),
            BridgeCommand::SubmitPrompt(p) => super::protocol::to_value(p),
            BridgeCommand::MonitorTab(p) => super::protocol::to_value(p),
            BridgeCommand::StopGeneration => serde_json::json!({}),
            BridgeCommand::CloseTab(p) => super::protocol::to_value(p),
            BridgeCommand::ApproveOrDeny(p) => super::protocol::to_value(p),
            BridgeCommand::Shutdown(p) => super::protocol::to_value(p),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::protocol::{decode_envelope, encode_envelope, envelope_for_command};

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
    fn unknown_command_kind_returns_protocol_error() {
        let err =
            BridgeCommand::decode("does-not-exist", serde_json::json!({})).expect_err("unknown");
        assert!(matches!(err, ProtocolError::UnknownCommand(_)));
    }

    #[test]
    fn upload_archive_payload_serializes_fresh_source_clone() {
        let payload = UploadArchivePayload {
            repo_url: "/tmp/source".into(),
            ref_name: "HEAD".into(),
            prefix: "source/".into(),
            archive_filename: "source.tar.gz".into(),
            tmp_parent: None,
            fresh_source_clone: true,
            delete_after_upload: true,
            confirm_selectors: Vec::new(),
            timeout_ms: 45_000,
            prompt: None,
            submit_timeout_ms: None,
        };
        let value = serde_json::to_value(&payload).expect("serialize");
        assert_eq!(value["fresh_source_clone"], true);
        assert!(
            value.get("prompt").is_none(),
            "prompt should be omitted when None"
        );
        let decoded: UploadArchivePayload = serde_json::from_value(value).expect("decode");
        assert!(decoded.fresh_source_clone);
        assert!(decoded.prompt.is_none());
    }
}

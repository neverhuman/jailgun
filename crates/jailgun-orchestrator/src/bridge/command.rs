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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UploadArchivePayload {
    pub repo_url: String,
    #[serde(default = "default_ref_name")]
    pub ref_name: String,
    pub prefix: String,
    pub archive_filename: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_archive_path: Option<String>,
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
pub struct AuthStatusPayload {
    pub chat_url: String,
    #[serde(default)]
    pub profile_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthBeginPayload {
    pub chat_url: String,
    pub email_hint: String,
    #[serde(default)]
    pub prefer_email_code: bool,
    #[serde(default)]
    pub profile_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthSelectEmailCodePayload {
    #[serde(default)]
    pub profile_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthSubmitCodePayload {
    pub code: String,
    #[serde(default)]
    pub profile_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthScreenshotPayload {
    pub path: String,
    #[serde(default)]
    pub profile_dir: Option<String>,
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
    AuthStatus(AuthStatusPayload),
    AuthBegin(AuthBeginPayload),
    AuthSelectEmailCode(AuthSelectEmailCodePayload),
    AuthSubmitCode(AuthSubmitCodePayload),
    AuthScreenshot(AuthScreenshotPayload),
    AuthCancel,
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
            BridgeCommand::AuthStatus(_) => "auth-status",
            BridgeCommand::AuthBegin(_) => "auth-begin",
            BridgeCommand::AuthSelectEmailCode(_) => "auth-select-email-code",
            BridgeCommand::AuthSubmitCode(_) => "auth-submit-code",
            BridgeCommand::AuthScreenshot(_) => "auth-screenshot",
            BridgeCommand::AuthCancel => "auth-cancel",
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
            BridgeCommand::AuthStatus(p) => super::protocol::to_value(p),
            BridgeCommand::AuthBegin(p) => super::protocol::to_value(p),
            BridgeCommand::AuthSelectEmailCode(p) => super::protocol::to_value(p),
            BridgeCommand::AuthSubmitCode(p) => super::protocol::to_value(p),
            BridgeCommand::AuthScreenshot(p) => super::protocol::to_value(p),
            BridgeCommand::AuthCancel => serde_json::json!({}),
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
            "auth-status" => BridgeCommand::AuthStatus(serde_json::from_value(payload)?),
            "auth-begin" => BridgeCommand::AuthBegin(serde_json::from_value(payload)?),
            "auth-select-email-code" => {
                BridgeCommand::AuthSelectEmailCode(serde_json::from_value(payload)?)
            }
            "auth-submit-code" => BridgeCommand::AuthSubmitCode(serde_json::from_value(payload)?),
            "auth-screenshot" => BridgeCommand::AuthScreenshot(serde_json::from_value(payload)?),
            "auth-cancel" => BridgeCommand::AuthCancel,
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
            profile_dir: Some("/tmp/profile".into()),
        });
        let envelope = envelope_for_command(&cmd, "run-test", "2026-05-31T12:00:00Z", Some(2));
        let line = encode_envelope(&envelope).expect("encode");
        assert!(line.ends_with('\n'));
        let decoded = decode_envelope(line.trim_end()).expect("decode");
        let typed = BridgeCommand::decode(&decoded.kind, decoded.payload).expect("typed");
        assert_eq!(typed, cmd);
    }

    #[test]
    fn roundtrip_auth_submit_code_command_redacts_by_type_not_payload() {
        let cmd = BridgeCommand::AuthSubmitCode(AuthSubmitCodePayload {
            code: "123456".into(),
            profile_dir: Some("/tmp/profile".into()),
        });
        let envelope = envelope_for_command(&cmd, "auth-run", "2026-05-31T12:00:00Z", None);
        let line = encode_envelope(&envelope).expect("encode");
        let decoded = decode_envelope(line.trim_end()).expect("decode");
        assert_eq!(decoded.kind, "auth-submit-code");
        let typed = BridgeCommand::decode(&decoded.kind, decoded.payload).expect("typed");
        assert_eq!(typed, cmd);
    }

    #[test]
    fn roundtrip_upload_archive_with_local_archive_path() {
        let cmd = BridgeCommand::UploadArchive(UploadArchivePayload {
            repo_url: "local://jailhard".into(),
            ref_name: "HEAD".into(),
            prefix: "source/".into(),
            archive_filename: "source.tar.gz".into(),
            local_archive_path: Some("/tmp/jailgun-hardening/source.tar.gz".into()),
            tmp_parent: None,
            delete_after_upload: true,
            confirm_selectors: Vec::new(),
            timeout_ms: 45_000,
        });
        let envelope = envelope_for_command(&cmd, "run-test", "2026-05-31T12:00:00Z", Some(1));
        let line = encode_envelope(&envelope).expect("encode");
        assert!(line.contains("local_archive_path"));
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
}

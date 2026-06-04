//! `BridgeEvent` enum and per-variant payload structs.

mod payload;

pub use payload::{
    ArchiveUploadedPayload, AuthActionNeededPayload, AuthCodeRequestedPayload,
    AuthCodeSubmittedPayload, AuthCompletePayload, AuthFailedPayload, AuthStatePayload,
    BridgeLogPayload, BridgeProfilePayload, BridgeReadyPayload, BridgeShuttingDownPayload,
    DownloadCompletePayload, DownloadStartedPayload, ErrorPayload, GenerationStoppedPayload,
    PromptPolicyAppliedPayload, PromptSubmittedPayload, RateLimitDetectedPayload,
    SessionExpiredPayload, TabClosedPayload, TabOpenedPayload, TabProgressKind, TabProgressPayload,
    TarDiscoveredPayload, ToolPromptDetectedPayload,
};

use super::protocol::{self, ProtocolError};

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
    AuthState(AuthStatePayload),
    AuthActionNeeded(AuthActionNeededPayload),
    AuthCodeRequested(AuthCodeRequestedPayload),
    AuthCodeSubmitted(AuthCodeSubmittedPayload),
    AuthComplete(AuthCompletePayload),
    AuthFailed(AuthFailedPayload),
    SessionExpired(SessionExpiredPayload),
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
            BridgeEvent::AuthState(_) => "auth-state",
            BridgeEvent::AuthActionNeeded(_) => "auth-action-needed",
            BridgeEvent::AuthCodeRequested(_) => "auth-code-requested",
            BridgeEvent::AuthCodeSubmitted(_) => "auth-code-submitted",
            BridgeEvent::AuthComplete(_) => "auth-complete",
            BridgeEvent::AuthFailed(_) => "auth-failed",
            BridgeEvent::SessionExpired(_) => "session-expired",
            BridgeEvent::Pong => "pong",
            BridgeEvent::BridgeShuttingDown(_) => "bridge-shutting-down",
            BridgeEvent::Error(_) => "error",
        }
    }

    pub fn payload(&self) -> serde_json::Value {
        match self {
            BridgeEvent::BridgeReady(p) => protocol::to_value(p),
            BridgeEvent::TabOpened(p) => protocol::to_value(p),
            BridgeEvent::ArchiveUploaded(p) => protocol::to_value(p),
            BridgeEvent::PromptSubmitted(p) => protocol::to_value(p),
            BridgeEvent::TabProgress(p) => protocol::to_value(p),
            BridgeEvent::TarDiscovered(p) => protocol::to_value(p),
            BridgeEvent::DownloadStarted(p) => protocol::to_value(p),
            BridgeEvent::DownloadComplete(p) => protocol::to_value(p),
            BridgeEvent::ToolPromptDetected(p) => protocol::to_value(p),
            BridgeEvent::PromptPolicyApplied(p) => protocol::to_value(p),
            BridgeEvent::RateLimitDetected(p) => protocol::to_value(p),
            BridgeEvent::GenerationStopped(p) => protocol::to_value(p),
            BridgeEvent::TabClosed(p) => protocol::to_value(p),
            BridgeEvent::BridgeLog(p) => protocol::to_value(p),
            BridgeEvent::AuthState(p) => protocol::to_value(p),
            BridgeEvent::AuthActionNeeded(p) => protocol::to_value(p),
            BridgeEvent::AuthCodeRequested(p) => protocol::to_value(p),
            BridgeEvent::AuthCodeSubmitted(p) => protocol::to_value(p),
            BridgeEvent::AuthComplete(p) => protocol::to_value(p),
            BridgeEvent::AuthFailed(p) => protocol::to_value(p),
            BridgeEvent::SessionExpired(p) => protocol::to_value(p),
            BridgeEvent::Pong => serde_json::json!({}),
            BridgeEvent::BridgeShuttingDown(p) => protocol::to_value(p),
            BridgeEvent::Error(p) => protocol::to_value(p),
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
            "auth-state" => BridgeEvent::AuthState(serde_json::from_value(payload)?),
            "auth-action-needed" => BridgeEvent::AuthActionNeeded(serde_json::from_value(payload)?),
            "auth-code-requested" => {
                BridgeEvent::AuthCodeRequested(serde_json::from_value(payload)?)
            }
            "auth-code-submitted" => {
                BridgeEvent::AuthCodeSubmitted(serde_json::from_value(payload)?)
            }
            "auth-complete" => BridgeEvent::AuthComplete(serde_json::from_value(payload)?),
            "auth-failed" => BridgeEvent::AuthFailed(serde_json::from_value(payload)?),
            "session-expired" => BridgeEvent::SessionExpired(serde_json::from_value(payload)?),
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
mod tests;

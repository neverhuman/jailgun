//! NDJSON wire envelope and framing helpers.
//!
//! Each line on the bridge child's stdin or stdout is a JSON object that
//! decodes to `Envelope<serde_json::Value>`. The `kind` field is the
//! discriminator; the `payload` field is the variant-specific body. Use
//! [`crate::bridge::BridgeCommand::decode`] and
//! [`crate::bridge::BridgeEvent::decode`] to lift a raw envelope into a typed
//! value.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{BridgeCommand, BridgeEvent};

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

pub(super) fn to_value<P: Serialize>(payload: &P) -> serde_json::Value {
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
}

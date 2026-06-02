//! JPCM/2.0 envelope shape jailgun writes to the JMCP outbox.
//!
//! The schema is the v1.0.0 spec in
//! `~/code/jmcp/tips/v6/JPCM_FINAL_PROTOCOL_SCHEMA_v1.0.0.json`.
//! We populate the required top-level fields with the minimal subset that
//! still validates and that a future JMCP impl can act on. Optional fields
//! (`evidence`, `observability`, `attention`, `privacy`, `extensions`) are
//! left off until we have a reason to populate them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

pub const SCHEMA_VERSION: &str = "jpcm/1.0.0";
pub const PRODUCER_NAME: &str = "jailgun";

/// Top-level JPCM/2.0 envelope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JmcpEnvelope {
    pub schema_version: String,
    pub envelope_id: String,
    pub event_type: EventType,
    pub event_time: String,
    pub producer: Producer,
    pub authority: Authority,
    pub task: TaskRef,
    pub routing: Routing,
    pub payload: Payload,
    pub integrity: Integrity,
}

impl JmcpEnvelope {
    /// Build an envelope for a jailgun notification payload. The integrity
    /// hash is computed over the serialized payload only — clients can
    /// re-verify by re-serializing `payload` and re-hashing.
    pub fn new(payload: Payload, task: TaskRef, routing: Routing) -> Self {
        let envelope_id = generate_envelope_id();
        let event_time = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
        let integrity = Integrity::for_payload(&payload);
        Self {
            schema_version: SCHEMA_VERSION.to_string(),
            envelope_id,
            event_type: EventType {
                domain: "observability".to_string(),
                action: "notification".to_string(),
                risk_tier: "R0_local_read".to_string(),
            },
            event_time,
            producer: Producer::default(),
            authority: Authority::default(),
            task,
            routing,
            payload,
            integrity,
        }
    }
}

/// `jpcm_<22-char urlsafe base64>` — matches the schema pattern
/// `^jpcm_[A-Za-z0-9_-]{16,96}$` and stays well under the 96 cap.
pub fn generate_envelope_id() -> String {
    let bytes = Uuid::new_v4().into_bytes();
    let mut encoded = base64_url_no_pad(&bytes);
    encoded.insert_str(0, "jpcm_");
    encoded
}

fn base64_url_no_pad(bytes: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((bytes.len() * 4).div_ceil(3));
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let combined = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARSET[((combined >> 18) & 0x3f) as usize] as char);
        out.push(CHARSET[((combined >> 12) & 0x3f) as usize] as char);
        if chunk.len() >= 2 {
            out.push(CHARSET[((combined >> 6) & 0x3f) as usize] as char);
        }
        if chunk.len() == 3 {
            out.push(CHARSET[(combined & 0x3f) as usize] as char);
        }
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventType {
    pub domain: String,
    pub action: String,
    pub risk_tier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Producer {
    pub name: String,
    pub version: String,
    pub kind: String,
}

impl Default for Producer {
    fn default() -> Self {
        Self {
            name: PRODUCER_NAME.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            kind: "native_service".to_string(),
        }
    }
}

/// Stubbed authority block. Jailgun has no lease grantor yet; the bridge and
/// any future JMCP impl can treat `lease_id = "jailgun-v0-pre-lease"` as the
/// signal that this envelope predates real lease enforcement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Authority {
    pub lease_id: String,
    pub autonomy_tier: String,
    pub granted_by: String,
    pub granted_at: String,
}

impl Default for Authority {
    fn default() -> Self {
        Self {
            lease_id: "jailgun-v0-pre-lease".to_string(),
            autonomy_tier: "R0_local_read".to_string(),
            granted_by: "jailgun".to_string(),
            granted_at: OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskRef {
    pub task_id: String,
    pub run_id: String,
    pub tab_id: Option<u16>,
}

impl TaskRef {
    pub fn for_run(run_id: &str, tab_id: Option<u16>) -> Self {
        let task_id = match tab_id {
            Some(tab) => format!("{run_id}-tab-{tab:02}"),
            None => run_id.to_string(),
        };
        Self {
            task_id,
            run_id: run_id.to_string(),
            tab_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Routing {
    pub stream: String,
    pub subject: String,
    pub destinations: Vec<String>,
}

impl Routing {
    pub fn notify_user() -> Self {
        Self {
            stream: "JPCM.USER".to_string(),
            subject: "jpcm.local.dev.user.notification.R0_local_read.jailgun".to_string(),
            destinations: vec!["user.telegram".to_string()],
        }
    }
}

/// Discriminated payload union. `kind` is the variant tag; everything else is
/// either common metadata or kind-specific data the bridge can render.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind")]
pub enum Payload {
    #[serde(rename = "jailgun.notify-commit")]
    NotifyCommit(NotifyCommitPayload),
    #[serde(rename = "jailgun.notify-event")]
    NotifyEvent(NotifyEventPayload),
    #[serde(rename = "jailgun.notify-text")]
    NotifyText(NotifyTextPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotifyCommitPayload {
    pub title: String,
    pub summary_emoji: String,
    pub body_markdown: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotifyEventPayload {
    pub title: String,
    pub summary_emoji: String,
    pub body_markdown: String,
    pub run_id: String,
    pub tab_id: Option<u16>,
    pub event_kind: String,
    pub event_severity: String,
    #[serde(default)]
    pub metrics: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotifyTextPayload {
    pub title: String,
    pub summary_emoji: String,
    pub body_markdown: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Integrity {
    pub algo: String,
    pub digest_hex: String,
}

impl Integrity {
    fn for_payload(payload: &Payload) -> Self {
        let serialized = serde_json::to_vec(payload).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(&serialized);
        let bytes = hasher.finalize();
        Self {
            algo: "sha256".to_string(),
            digest_hex: hex_lower(&bytes),
        }
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_payload() -> Payload {
        Payload::NotifyText(NotifyTextPayload {
            title: "hello".into(),
            summary_emoji: "👋".into(),
            body_markdown: "hello world".into(),
        })
    }

    #[test]
    fn envelope_carries_schema_version() {
        let env = JmcpEnvelope::new(
            dummy_payload(),
            TaskRef::for_run("run-A", Some(3)),
            Routing::notify_user(),
        );
        assert_eq!(env.schema_version, "jpcm/1.0.0");
        assert!(env.envelope_id.starts_with("jpcm_"));
        assert_eq!(env.task.task_id, "run-A-tab-03");
    }

    #[test]
    fn envelope_id_matches_jpcm_pattern() {
        let id = generate_envelope_id();
        let suffix = id.strip_prefix("jpcm_").unwrap();
        assert!(suffix.len() >= 16);
        assert!(suffix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
    }

    #[test]
    fn integrity_digest_is_sha256_hex() {
        let env = JmcpEnvelope::new(
            dummy_payload(),
            TaskRef::for_run("run-A", None),
            Routing::notify_user(),
        );
        assert_eq!(env.integrity.algo, "sha256");
        assert_eq!(env.integrity.digest_hex.len(), 64);
        assert!(env
            .integrity
            .digest_hex
            .chars()
            .all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn payload_round_trips_through_json() {
        let env = JmcpEnvelope::new(
            dummy_payload(),
            TaskRef::for_run("run-A", Some(1)),
            Routing::notify_user(),
        );
        let json = serde_json::to_string(&env).expect("serialize");
        let back: JmcpEnvelope = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(env, back);
    }
}

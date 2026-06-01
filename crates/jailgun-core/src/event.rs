use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EventKind {
    RunStarted,
    TabOpened,
    PromptSubmitted,
    TarDiscovered,
    DownloadReceipt,
    DeployQueued,
    RemoteSafety,
    DeployFinished,
    PromptPolicy,
    RateLimitDetected,
    BrowserLog,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JailgunEvent {
    pub run_id: String,
    pub tab_id: Option<u16>,
    pub timestamp: String,
    pub kind: EventKind,
    pub severity: Severity,
    pub message: String,
    pub fields: BTreeMap<String, String>,
}

impl JailgunEvent {
    pub fn new(run_id: impl Into<String>, kind: EventKind, message: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
            tab_id: None,
            timestamp: OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string()),
            kind,
            severity: Severity::Info,
            message: message.into(),
            fields: BTreeMap::new(),
        }
    }

    pub fn with_tab(mut self, tab_id: u16) -> Self {
        self.tab_id = Some(tab_id);
        self
    }

    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_websocket_event_contract() {
        let event = JailgunEvent::new("run-1", EventKind::RemoteSafety, "preserved")
            .with_tab(2)
            .with_field("policy", "preserve-reset");

        let json = serde_json::to_value(&event).expect("event serializes");
        assert_eq!(json["run_id"], "run-1");
        assert_eq!(json["tab_id"], 2);
        assert_eq!(json["kind"], "remote-safety");
        assert_eq!(json["fields"]["policy"], "preserve-reset");
    }
}

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeReadyPayload {
    pub node_version: String,
    pub playwright_version: String,
    pub browser: String,
    pub browser_version: String,
    #[serde(default)]
    pub cdp_url: Option<String>,
    #[serde(default)]
    pub managed_chrome_started: Option<bool>,
    #[serde(default)]
    pub profile_count: Option<u16>,
    #[serde(default)]
    pub profiles: Vec<BridgeProfilePayload>,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeProfilePayload {
    pub slot: u16,
    #[serde(default)]
    pub profile_name: String,
    #[serde(default)]
    pub profile_dir: String,
    #[serde(default)]
    pub state_dir: String,
    #[serde(default)]
    pub cdp_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TabOpenedPayload {
    pub page_url: String,
    pub page_id: String,
    #[serde(default)]
    pub browser_profile: String,
    #[serde(default)]
    pub browser_profile_dir: String,
    #[serde(default)]
    pub browser_slot: Option<u16>,
    #[serde(default)]
    pub cdp_url: String,
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
    pub file_kind: Option<String>,
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
pub struct AuthStatePayload {
    pub state: String,
    pub page_url: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub composer_detected: bool,
    #[serde(default)]
    pub code_requested: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthActionNeededPayload {
    pub action: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthCodeRequestedPayload {
    pub channel: String,
    #[serde(default)]
    pub destination_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthCodeSubmittedPayload {
    pub accepted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthCompletePayload {
    pub page_url: String,
    #[serde(default)]
    pub composer_detected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthFailedPayload {
    pub reason: String,
    #[serde(default)]
    pub manual_browser_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionExpiredPayload {
    pub page_url: String,
    pub reason: String,
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

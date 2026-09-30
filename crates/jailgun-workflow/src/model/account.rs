use super::ModelSelection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AccountIdentity {
    pub id: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AccountObservation {
    pub identity: AccountIdentity,
    pub model: String,
    pub available_models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AccountSession {
    pub account_id: String,
    pub email: String,
    pub lifecycle: String,
    pub model: Option<ModelSelection>,
    pub observation: Option<AccountObservation>,
    pub observed_ms: Option<i64>,
    pub login_expires_ms: Option<i64>,
    pub error_code: Option<String>,
    /// Ephemeral supervisor observation; never persisted as account readiness.
    #[serde(default)]
    pub login_view_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectAccount {
    pub email: String,
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfirmAccount {
    pub identity: AccountIdentity,
    pub model: ModelSelection,
}

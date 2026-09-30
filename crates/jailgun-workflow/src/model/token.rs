use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IssueToken {
    pub name: String,
    /// Explicit account IDs; future accounts are never added implicitly.
    pub account_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct TokenMetadata {
    pub id: String,
    pub name: String,
    pub account_ids: Vec<String>,
    pub created_ms: i64,
    pub revoked_ms: Option<i64>,
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct CreatedToken {
    pub metadata: TokenMetadata,
    /// Returned once. Only its SHA-256 digest is retained by the installation.
    pub secret: String,
}

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentError {
    pub code: &'static str,
    pub purpose: &'static str,
    pub reason: String,
    #[serde(rename = "common fixes")]
    pub common_fixes: Vec<&'static str>,
    pub docs_url: &'static str,
    pub repair_hint: &'static str,
}

impl AgentError {
    pub fn new(
        code: &'static str,
        purpose: &'static str,
        reason: impl Into<String>,
        common_fixes: Vec<&'static str>,
        docs_url: &'static str,
        repair_hint: &'static str,
    ) -> Self {
        Self {
            code,
            purpose,
            reason: reason.into(),
            common_fixes,
            docs_url,
            repair_hint,
        }
    }
}

pub trait AgentErrorExt {
    fn agent_error(&self) -> AgentError;
}

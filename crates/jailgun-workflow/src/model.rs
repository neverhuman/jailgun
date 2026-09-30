use crate::{Error, Result};
use serde::{Deserialize, Serialize};
mod account;
pub use account::*;
mod token;
pub use token::*;

pub const SUMMARY_CHAR_LIMIT: usize = 6_000;
pub const STAGE_CHAR_LIMIT: usize = 128_000;
pub const RESPONSE_TIMEOUT_MS: i64 = 30 * 60 * 1_000;
pub const RUN_DEADLINE_MS: i64 = 3 * 60 * 60 * 1_000;
pub const SUBMISSION_SPACING_MS: i64 = 30_000;
pub const MAX_JITTER_MS: i64 = 5_000;
pub const TEMPLATE_VERSION: &str = "1";

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Criterion {
    #[schemars(length(min = 1, max = 120))]
    pub name: String,
    #[schemars(range(min = 1, max = 100))]
    pub weight: u16,
}

pub fn default_criteria() -> Vec<Criterion> {
    [
        ("Usefulness", 30),
        ("Feasibility", 25),
        ("Novelty", 20),
        ("Supporting evidence", 15),
        ("Risk management", 10),
    ]
    .into_iter()
    .map(|(name, weight)| Criterion {
        name: name.into(),
        weight,
    })
    .collect()
}

fn five() -> u16 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConceptRequest {
    #[schemars(length(min = 1, max = 64_000))]
    pub concept: String,
    pub account_id: String,
    #[serde(default = "five")]
    #[schemars(range(min = 5, max = 10))]
    pub candidate_count: u16,
    #[serde(default)]
    #[schemars(length(max = 16_000))]
    pub constraints: String,
    #[serde(default = "default_criteria")]
    #[schemars(length(min = 1, max = 10))]
    pub criteria: Vec<Criterion>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunAccepted {
    pub run_id: String,
    pub status: String,
    pub run_url: String,
    pub result_url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResumeRequest {
    #[serde(default)]
    pub allow_incomplete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FinalResult {
    /// Accepted final text, populated when reading historical manifests too.
    #[serde(default)]
    pub final_artifact: Option<Artifact>,
    pub schema_version: u16,
    pub run_id: String,
    pub status: String,
    pub request: ConceptRequest,
    pub configuration: ConfigurationSnapshot,
    pub excluded_candidates: Vec<u16>,
    pub evaluation_notice: String,
    pub artifacts: Vec<Artifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RankedCandidate {
    pub weighted_score: f64,
    pub evaluation: CandidateEvaluation,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Comparison {
    pub basis: String,
    pub ranking: Vec<RankedCandidate>,
}

impl ConceptRequest {
    pub fn validate(&self) -> Result<()> {
        let invalid = |message| {
            Error::action(
                "invalid-request",
                message,
                "Correct the concept request and submit again.",
            )
        };
        if self.concept.trim().is_empty()
            || self.concept.chars().count() > 64_000
            || self.constraints.chars().count() > 16_000
        {
            return Err(invalid(
                "Concept must contain 1–64000 characters; constraints allow at most 16000.",
            ));
        }
        if !(5..=10).contains(&self.candidate_count) {
            return Err(invalid("Candidate count must be between five and ten."));
        }
        if jailgun_core::validate_account_id(&self.account_id).is_err()
            || self.idempotency_key.trim().is_empty()
            || self.idempotency_key.len() > 128
        {
            return Err(invalid(
                "A valid account ID and an idempotency key of at most 128 bytes are required.",
            ));
        }
        let mut names = std::collections::HashSet::new();
        if self.criteria.is_empty()
            || self.criteria.len() > 10
            || self
                .criteria
                .iter()
                .map(|c| u32::from(c.weight))
                .sum::<u32>()
                != 100
            || self.criteria.iter().any(|c| {
                c.name.trim().is_empty()
                    || c.name.chars().count() > 120
                    || c.weight == 0
                    || !names.insert(c.name.trim().to_lowercase())
            })
        {
            return Err(invalid(
                "Use one to ten uniquely named criteria with positive weights totaling 100.",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ModelSelection {
    Current,
    Specific { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub struct ConfigurationSnapshot {
    pub template_version: String,
    pub perspectives: Vec<String>,
    pub model: ModelSelection,
    pub submission_spacing_ms: i64,
    pub max_jitter_ms: i64,
    pub response_timeout_ms: i64,
    pub overall_deadline_ms: i64,
    pub max_attempts: u16,
    pub summary_char_limit: usize,
    pub stage_char_limit: usize,
}

impl ConfigurationSnapshot {
    pub fn new(count: u16, model: ModelSelection) -> Self {
        Self {
            template_version: TEMPLATE_VERSION.into(),
            perspectives: crate::prompts::PERSPECTIVES
                .iter()
                .take(usize::from(count))
                .map(|s| (*s).into())
                .collect(),
            model,
            submission_spacing_ms: SUBMISSION_SPACING_MS,
            max_jitter_ms: MAX_JITTER_MS,
            response_timeout_ms: RESPONSE_TIMEOUT_MS,
            overall_deadline_ms: RUN_DEADLINE_MS,
            max_attempts: 2,
            summary_char_limit: SUMMARY_CHAR_LIMIT,
            stage_char_limit: STAGE_CHAR_LIMIT,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CandidateSummary {
    pub proposal: String,
    pub strengths: Vec<String>,
    pub weaknesses: Vec<String>,
    pub assumptions: Vec<String>,
    pub next_steps: Vec<String>,
}

impl CandidateSummary {
    pub fn validate(&self) -> Result<()> {
        if self.proposal.trim().is_empty()
            || self.strengths.is_empty()
            || self.weaknesses.is_empty()
            || self.assumptions.is_empty()
            || self.next_steps.is_empty()
            || self
                .strengths
                .iter()
                .chain(&self.weaknesses)
                .chain(&self.assumptions)
                .chain(&self.next_steps)
                .any(|s| s.trim().is_empty())
        {
            return Err(Error::action(
                "invalid-summary",
                "The candidate summary is incomplete.",
                "Retry the candidate with a valid structured summary.",
            ));
        }
        if serde_json::to_string(self)?.chars().count() > SUMMARY_CHAR_LIMIT {
            return Err(Error::action("context-too-large", "A candidate summary exceeds 6000 characters.", "Retry with a shorter structured summary; full response text is retained separately."));
        }
        Ok(())
    }
}

mod run;
pub use run::*;

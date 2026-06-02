use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ToolPromptAction {
    Read,
    Search,
    Write,
    Commit,
    CreateTree,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolPrompt {
    pub provider: String,
    pub repository: Option<String>,
    pub action: ToolPromptAction,
    pub label: String,
    pub context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptPolicy {
    pub deny_github_write_by_default: bool,
    #[serde(default)]
    pub allow_write_prompts: bool,
    pub allow_info_prompts: bool,
    pub allowed_repositories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PromptDecision {
    Deny { reason: String },
    AllowInfo { reason: String },
    AllowWrite { reason: String },
    Ignore { reason: String },
}

impl PromptPolicy {
    pub fn decide(&self, prompt: &ToolPrompt) -> PromptDecision {
        if !prompt.provider.eq_ignore_ascii_case("github") {
            return PromptDecision::Ignore {
                reason: "non-github-tool".into(),
            };
        }

        let repository_allowed = match prompt.repository.as_ref() {
            Some(repo) => {
                self.allowed_repositories.is_empty()
                    || self
                        .allowed_repositories
                        .iter()
                        .any(|allowed| allowed == repo)
            }
            None => true,
        };
        if !repository_allowed {
            return PromptDecision::Deny {
                reason: "repository-not-allowed".into(),
            };
        }

        match prompt.action {
            ToolPromptAction::Read | ToolPromptAction::Search => {
                if self.allow_info_prompts {
                    PromptDecision::AllowInfo {
                        reason: "explicit-info-policy".into(),
                    }
                } else {
                    PromptDecision::Deny {
                        reason: "info-prompts-not-enabled".into(),
                    }
                }
            }
            ToolPromptAction::Write | ToolPromptAction::Commit | ToolPromptAction::CreateTree => {
                if self.deny_github_write_by_default {
                    PromptDecision::Deny {
                        reason: "github-write-denied-by-default".into(),
                    }
                } else if self.allow_write_prompts {
                    PromptDecision::AllowWrite {
                        reason: "explicit-github-write-policy".into(),
                    }
                } else {
                    PromptDecision::Deny {
                        reason: "github-write-requires-explicit-narrow-policy".into(),
                    }
                }
            }
            ToolPromptAction::Unknown => PromptDecision::Deny {
                reason: "unknown-github-tool-action".into(),
            },
        }
    }
}

impl Default for PromptPolicy {
    fn default() -> Self {
        Self {
            deny_github_write_by_default: true,
            allow_write_prompts: false,
            allow_info_prompts: false,
            allowed_repositories: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn github_prompt(action: ToolPromptAction) -> ToolPrompt {
        ToolPrompt {
            provider: "github".into(),
            repository: Some("org/example".into()),
            action,
            label: "tool prompt".into(),
            context: "GitHub asks for access".into(),
        }
    }

    #[test]
    fn denies_github_write_prompts_by_default() {
        let policy = PromptPolicy::default();
        let decision = policy.decide(&github_prompt(ToolPromptAction::CreateTree));
        assert!(matches!(decision, PromptDecision::Deny { .. }));
    }

    #[test]
    fn allows_information_prompts_only_when_enabled() {
        let mut policy = PromptPolicy::default();
        let denied = policy.decide(&github_prompt(ToolPromptAction::Read));
        assert!(matches!(denied, PromptDecision::Deny { .. }));

        policy.allow_info_prompts = true;
        let allowed = policy.decide(&github_prompt(ToolPromptAction::Read));
        assert!(matches!(allowed, PromptDecision::AllowInfo { .. }));
    }

    #[test]
    fn allows_github_write_only_with_explicit_policy() {
        let mut policy = PromptPolicy::default();
        let denied = policy.decide(&github_prompt(ToolPromptAction::Write));
        assert!(matches!(denied, PromptDecision::Deny { .. }));

        policy.deny_github_write_by_default = false;
        policy.allow_write_prompts = true;
        policy.allowed_repositories = vec!["org/example".into()];
        let allowed = policy.decide(&github_prompt(ToolPromptAction::Write));
        assert!(matches!(allowed, PromptDecision::AllowWrite { .. }));
    }
}

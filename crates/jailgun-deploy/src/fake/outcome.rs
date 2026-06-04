#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FakeOutcome {
    Success,
    ShaMismatch,
    CommandFail,
    CiFail,
    CleanupDivergent,
}

impl FakeOutcome {
    pub fn from_env() -> Self {
        match std::env::var("JAILGUN_FAKE_REMOTE_RESULT").ok().as_deref() {
            Some("sha-mismatch") => FakeOutcome::ShaMismatch,
            Some("command-fail") => FakeOutcome::CommandFail,
            Some("ci-fail") => FakeOutcome::CiFail,
            Some("cleanup-divergent") => FakeOutcome::CleanupDivergent,
            _ => FakeOutcome::Success,
        }
    }
}

#[derive(Debug)]
pub struct FakeBus {
    pub outcome: FakeOutcome,
}

impl FakeBus {
    pub fn from_env() -> Self {
        Self {
            outcome: FakeOutcome::from_env(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_from_env_defaults_to_success() {
        // SAFETY: tests run single-threaded for this env-touching block.
        std::env::remove_var("JAILGUN_FAKE_REMOTE_RESULT");
        assert_eq!(FakeOutcome::from_env(), FakeOutcome::Success);
        std::env::set_var("JAILGUN_FAKE_REMOTE_RESULT", "command-fail");
        assert_eq!(FakeOutcome::from_env(), FakeOutcome::CommandFail);
        std::env::set_var("JAILGUN_FAKE_REMOTE_RESULT", "ci-fail");
        assert_eq!(FakeOutcome::from_env(), FakeOutcome::CiFail);
        std::env::remove_var("JAILGUN_FAKE_REMOTE_RESULT");
    }
}

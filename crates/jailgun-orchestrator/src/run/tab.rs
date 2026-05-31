//! Tab-level state machine.
//!
//! `TabActor` owns one ChatGPT page in the bridge. The orchestrator routes
//! per-tab `BridgeEvent`s into a `TabActor` and the actor reacts by
//! transitioning state, emitting `JailgunEvent`s, and (when appropriate)
//! sending commands back to the bridge.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum TabState {
    Opening,
    Submitted,
    Generating,
    TarDiscovered,
    Downloading,
    Downloaded,
    Closed,
    Deploying,
    DeployComplete,
    Failed { reason: String },
}

impl TabState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, TabState::DeployComplete | TabState::Failed { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabTrigger {
    TabOpened,
    ArchiveUploaded,
    PromptSubmitted,
    FirstProgress,
    TarLinkFound,
    DownloadStarted,
    DownloadComplete,
    GenerationStopped,
    TabClosed,
    DeployQueued,
    DeployFinishedSuccess,
    DeployFinishedFailure,
    ErrorFatal,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid transition from {from:?} on {trigger:?}")]
pub struct TabTransitionError {
    pub from: TabState,
    pub trigger: TabTrigger,
}

impl TabState {
    pub fn next(
        self,
        trigger: TabTrigger,
        reason: Option<String>,
    ) -> Result<TabState, TabTransitionError> {
        use TabState::*;
        use TabTrigger::*;
        let next = match (&self, trigger) {
            (Opening, TabOpened) => Opening,
            (Opening, ArchiveUploaded) => Opening,
            (Opening, PromptSubmitted) => Submitted,
            (Submitted, FirstProgress) => Generating,
            (Submitted | Generating, TarLinkFound) => TarDiscovered,
            (TarDiscovered, DownloadStarted) => Downloading,
            (Downloading, DownloadComplete) => Downloaded,
            (Downloaded, GenerationStopped) => Downloaded,
            (Downloaded, TabClosed) => Closed,
            (Closed, DeployQueued) => Deploying,
            (Deploying, DeployFinishedSuccess) => DeployComplete,
            (Deploying, DeployFinishedFailure) => Failed {
                reason: reason.unwrap_or_else(|| "deploy failed".into()),
            },
            (_, ErrorFatal) => Failed {
                reason: reason.unwrap_or_else(|| "fatal bridge error".into()),
            },
            (from, trigger) => {
                return Err(TabTransitionError {
                    from: from.clone(),
                    trigger,
                });
            }
        };
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(state: TabState, trigger: TabTrigger) -> TabState {
        state.next(trigger, None).expect("legal transition")
    }

    #[test]
    fn happy_path_reaches_deploy_complete() {
        let mut s = TabState::Opening;
        s = step(s, TabTrigger::TabOpened);
        s = step(s, TabTrigger::ArchiveUploaded);
        s = step(s, TabTrigger::PromptSubmitted);
        assert_eq!(s, TabState::Submitted);
        s = step(s, TabTrigger::FirstProgress);
        assert_eq!(s, TabState::Generating);
        s = step(s, TabTrigger::TarLinkFound);
        assert_eq!(s, TabState::TarDiscovered);
        s = step(s, TabTrigger::DownloadStarted);
        assert_eq!(s, TabState::Downloading);
        s = step(s, TabTrigger::DownloadComplete);
        s = step(s, TabTrigger::GenerationStopped);
        s = step(s, TabTrigger::TabClosed);
        assert_eq!(s, TabState::Closed);
        s = step(s, TabTrigger::DeployQueued);
        assert_eq!(s, TabState::Deploying);
        s = step(s, TabTrigger::DeployFinishedSuccess);
        assert_eq!(s, TabState::DeployComplete);
        assert!(s.is_terminal());
    }

    #[test]
    fn fatal_error_from_any_non_terminal_state_marks_failed() {
        let s = TabState::Submitted;
        let next = s
            .next(TabTrigger::ErrorFatal, Some("bridge died".into()))
            .expect("error transition");
        assert_eq!(
            next,
            TabState::Failed {
                reason: "bridge died".into()
            }
        );
    }

    #[test]
    fn illegal_transition_returns_error() {
        let err = TabState::Closed
            .next(TabTrigger::TabOpened, None)
            .expect_err("illegal");
        assert_eq!(err.trigger, TabTrigger::TabOpened);
    }
}

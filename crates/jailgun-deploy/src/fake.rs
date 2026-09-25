//! Env-driven fake backends for CI end-to-end testing.
//!
//! Activated by `JAILGUN_FAKE_REMOTE=1`. The fake outcome is selected via
//! `JAILGUN_FAKE_REMOTE_RESULT`:
//!
//! - `success` (default): cleanup is already-synced, upload sha matches,
//!   remote job phase=done, CI=skipped.
//! - `sha-mismatch`: remote sha returns a deterministic wrong value.
//! - `command-fail`: launcher phase=failed-preserved with preserved_ref +
//!   preserved_stash_ref set.
//! - `ci-fail`: deploy succeeds but CI tracker reports Failed.
//! - `cleanup-divergent`: cleanup sees a divergent HEAD; preserve-reset path
//!   exercised.
//!
//! The trait impls live behind the `fake-backends` Cargo feature so they
//! never ship in production binaries unless the feature is explicitly enabled.

mod ci_tracker;
mod git;
mod job;
mod outcome;
mod receipt;
mod upload;

pub use ci_tracker::FakeCiTracker;
pub use git::FakeRemoteGit;
pub use job::FakeRemoteJob;
pub use outcome::{FakeBus, FakeOutcome};
pub use receipt::FakeReceiptWriter;
pub use upload::FakeRemoteUpload;

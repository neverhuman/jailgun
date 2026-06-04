use std::{collections::VecDeque, path::PathBuf, sync::Mutex};

use async_trait::async_trait;

use crate::cleanup::{CleanupError, CleanupReceipt, RemoteGitBackend, RemoteSnapshot};

use super::FakeOutcome;

pub struct FakeRemoteGit {
    outcome: FakeOutcome,
    snapshots: Mutex<VecDeque<RemoteSnapshot>>,
    receipt_root: PathBuf,
}

impl FakeRemoteGit {
    pub fn new(outcome: FakeOutcome, receipt_root: PathBuf) -> Self {
        let snapshots: VecDeque<RemoteSnapshot> = match outcome {
            FakeOutcome::CleanupDivergent => VecDeque::from(vec![
                RemoteSnapshot::clean("head-fake-a", "head-fake-b"),
                RemoteSnapshot::clean("head-fake-a", "head-fake-b"),
                RemoteSnapshot::clean("head-fake-b", "head-fake-b"),
            ]),
            _ => VecDeque::from(vec![RemoteSnapshot::clean("head-fake-a", "head-fake-a")]),
        };
        Self {
            outcome,
            snapshots: Mutex::new(snapshots),
            receipt_root,
        }
    }

    fn synthesize_receipt_path(&self, receipt: &CleanupReceipt) -> PathBuf {
        let tab = receipt
            .tab_id
            .map(|id| format!("tab-{id:02}"))
            .unwrap_or_else(|| "no-tab".to_string());
        self.receipt_root
            .join(&receipt.run_id)
            .join(format!("{}-{tab}-cleanup-fake.json", receipt.run_id))
    }
}

#[async_trait]
impl RemoteGitBackend for FakeRemoteGit {
    async fn snapshot(&mut self, _remote_dir: &str) -> Result<RemoteSnapshot, CleanupError> {
        let mut guard = self
            .snapshots
            .lock()
            .map_err(|_| CleanupError::Backend("poisoned snapshot lock".into()))?;
        guard
            .pop_front()
            .or_else(|| guard.back().cloned())
            .ok_or_else(|| CleanupError::Backend("no fake snapshot".into()))
    }

    async fn fetch_origin(&mut self, _remote_dir: &str) -> Result<(), CleanupError> {
        Ok(())
    }

    async fn create_ref(
        &mut self,
        _remote_dir: &str,
        _ref_name: &str,
        _sha: &str,
    ) -> Result<(), CleanupError> {
        Ok(())
    }

    async fn write_receipt(&mut self, receipt: &CleanupReceipt) -> Result<PathBuf, CleanupError> {
        Ok(receipt
            .receipt_path
            .clone()
            .unwrap_or_else(|| self.synthesize_receipt_path(receipt)))
    }

    async fn reset_hard(&mut self, _remote_dir: &str, _target: &str) -> Result<(), CleanupError> {
        if matches!(self.outcome, FakeOutcome::CleanupDivergent) {
            if let Ok(mut guard) = self.snapshots.lock() {
                guard.clear();
                guard.push_back(RemoteSnapshot::clean("head-fake-b", "head-fake-b"));
            }
        }
        Ok(())
    }
}

use std::path::PathBuf;

#[cfg(test)]
use std::collections::VecDeque;

use async_trait::async_trait;
use jailgun_core::CleanupPolicy;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteSnapshot {
    pub head: Option<String>,
    pub origin_main: Option<String>,
    pub status_short: String,
}

impl RemoteSnapshot {
    pub fn clean(head: &str, origin_main: &str) -> Self {
        Self {
            head: Some(head.into()),
            origin_main: Some(origin_main.into()),
            status_short: String::new(),
        }
    }

    pub fn dirty(head: &str, origin_main: &str, status_short: &str) -> Self {
        Self {
            head: Some(head.into()),
            origin_main: Some(origin_main.into()),
            status_short: status_short.into(),
        }
    }

    pub fn is_clean(&self) -> bool {
        self.status_short.trim().is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CleanupOutcome {
    AlreadySynced,
    BlockedDivergent,
    PreservedReset,
    Adopted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CleanupRequest {
    pub run_id: String,
    pub tab_id: Option<u16>,
    pub remote_host: String,
    pub remote_dir: String,
    pub policy: CleanupPolicy,
    pub receipt_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CleanupReceipt {
    pub run_id: String,
    pub tab_id: Option<u16>,
    pub remote_host: String,
    pub remote_dir: String,
    pub policy: CleanupPolicy,
    pub outcome: CleanupOutcome,
    pub timestamp: String,
    pub initial_head: Option<String>,
    pub initial_origin_main: Option<String>,
    pub preserved_ref: Option<String>,
    pub preserved_sha: Option<String>,
    pub reset_to: Option<String>,
    pub final_head: Option<String>,
    pub final_status_short: String,
    pub receipt_path: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub enum CleanupError {
    #[error("remote checkout is dirty; refusing cleanup")]
    DirtyRemote { status_short: String },
    #[error("remote origin/main is missing")]
    MissingOriginMain,
    #[error("remote HEAD is missing")]
    MissingHead,
    #[error("remote HEAD differs from origin/main and policy is block")]
    DivergentBlocked { head: String, origin_main: String },
    #[error("preservation ref creation failed: {0}")]
    PreserveRef(String),
    #[error("preservation receipt write failed: {0}")]
    Receipt(String),
    #[error("remote reset failed: {0}")]
    Reset(String),
    #[error("remote fetch failed: {0}")]
    Fetch(String),
    #[error("backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait RemoteGitBackend {
    async fn snapshot(&mut self, remote_dir: &str) -> Result<RemoteSnapshot, CleanupError>;
    async fn fetch_origin(&mut self, remote_dir: &str) -> Result<(), CleanupError>;
    async fn create_ref(
        &mut self,
        remote_dir: &str,
        ref_name: &str,
        sha: &str,
    ) -> Result<(), CleanupError>;
    async fn write_receipt(&mut self, receipt: &CleanupReceipt) -> Result<PathBuf, CleanupError>;
    async fn reset_hard(&mut self, remote_dir: &str, target: &str) -> Result<(), CleanupError>;
}

pub async fn cleanup_remote_checkout<B: RemoteGitBackend + Send>(
    backend: &mut B,
    request: CleanupRequest,
) -> Result<CleanupReceipt, CleanupError> {
    let initial = backend.snapshot(&request.remote_dir).await?;
    if !initial.is_clean() {
        return Err(CleanupError::DirtyRemote {
            status_short: initial.status_short,
        });
    }
    let head = initial.head.clone().ok_or(CleanupError::MissingHead)?;
    let origin_main = initial
        .origin_main
        .clone()
        .ok_or(CleanupError::MissingOriginMain)?;
    if head == origin_main {
        let mut receipt = base_receipt(&request, CleanupOutcome::AlreadySynced, &initial);
        receipt.final_head = initial.head;
        receipt.final_status_short = initial.status_short;
        let path = backend.write_receipt(&receipt).await?;
        receipt.receipt_path = Some(path);
        return Ok(receipt);
    }

    match request.policy {
        CleanupPolicy::Block => Err(CleanupError::DivergentBlocked { head, origin_main }),
        CleanupPolicy::Adopt => {
            let mut receipt = base_receipt(&request, CleanupOutcome::Adopted, &initial);
            receipt.final_head = initial.head;
            receipt.final_status_short = initial.status_short;
            let path = backend.write_receipt(&receipt).await?;
            receipt.receipt_path = Some(path);
            Ok(receipt)
        }
        CleanupPolicy::PreserveReset => {
            preserve_reset(backend, request, initial, head, origin_main).await
        }
    }
}

async fn preserve_reset<B: RemoteGitBackend + Send>(
    backend: &mut B,
    request: CleanupRequest,
    initial: RemoteSnapshot,
    head: String,
    origin_main: String,
) -> Result<CleanupReceipt, CleanupError> {
    let timestamp = timestamp();
    let ref_name = format!(
        "refs/heads/jailgun-preserved/{}-{}",
        sanitize_ref_fragment(&request.run_id),
        timestamp.replace([':', '.'], "-")
    );
    backend
        .create_ref(&request.remote_dir, &ref_name, &head)
        .await
        .map_err(|error| CleanupError::PreserveRef(error.to_string()))?;

    let mut receipt = base_receipt_with_timestamp(
        &request,
        CleanupOutcome::PreservedReset,
        &initial,
        timestamp,
    );
    receipt.preserved_ref = Some(ref_name);
    receipt.preserved_sha = Some(head.clone());
    receipt.reset_to = Some(origin_main);
    let receipt_path = backend
        .write_receipt(&receipt)
        .await
        .map_err(|error| CleanupError::Receipt(error.to_string()))?;
    receipt.receipt_path = Some(receipt_path);

    backend
        .fetch_origin(&request.remote_dir)
        .await
        .map_err(|error| CleanupError::Fetch(error.to_string()))?;
    let after_fetch = backend.snapshot(&request.remote_dir).await?;
    let reset_to = after_fetch
        .origin_main
        .clone()
        .ok_or(CleanupError::MissingOriginMain)?;
    receipt.reset_to = Some(reset_to.clone());

    backend
        .reset_hard(&request.remote_dir, &reset_to)
        .await
        .map_err(|error| CleanupError::Reset(error.to_string()))?;
    let final_snapshot = backend.snapshot(&request.remote_dir).await?;
    if !final_snapshot.is_clean() {
        return Err(CleanupError::Reset("reset left checkout dirty".into()));
    }
    if final_snapshot.head.as_deref() != Some(reset_to.as_str()) {
        return Err(CleanupError::Reset(
            "reset target was not checked out".into(),
        ));
    }

    receipt.final_head = final_snapshot.head;
    receipt.final_status_short = final_snapshot.status_short;
    let final_path = backend
        .write_receipt(&receipt)
        .await
        .map_err(|error| CleanupError::Receipt(error.to_string()))?;
    receipt.receipt_path = Some(final_path);
    Ok(receipt)
}

fn base_receipt(
    request: &CleanupRequest,
    outcome: CleanupOutcome,
    initial: &RemoteSnapshot,
) -> CleanupReceipt {
    base_receipt_with_timestamp(request, outcome, initial, timestamp())
}

fn base_receipt_with_timestamp(
    request: &CleanupRequest,
    outcome: CleanupOutcome,
    initial: &RemoteSnapshot,
    timestamp: String,
) -> CleanupReceipt {
    let receipt_path = cleanup_receipt_path(request, &timestamp);
    CleanupReceipt {
        run_id: request.run_id.clone(),
        tab_id: request.tab_id,
        remote_host: request.remote_host.clone(),
        remote_dir: request.remote_dir.clone(),
        policy: request.policy,
        outcome,
        timestamp,
        initial_head: initial.head.clone(),
        initial_origin_main: initial.origin_main.clone(),
        preserved_ref: None,
        preserved_sha: None,
        reset_to: None,
        final_head: None,
        final_status_short: String::new(),
        receipt_path: Some(receipt_path),
    }
}

fn cleanup_receipt_path(request: &CleanupRequest, timestamp: &str) -> PathBuf {
    let run_id = sanitize_ref_fragment(&request.run_id);
    let tab = request
        .tab_id
        .map(|tab_id| format!("tab-{tab_id}"))
        .unwrap_or_else(|| "no-tab".to_string());
    request.receipt_dir.join(&run_id).join(format!(
        "{}-{}-{}-remote-cleanup.json",
        run_id,
        tab,
        sanitize_ref_fragment(timestamp)
    ))
}

fn timestamp() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

fn sanitize_ref_fragment(value: &str) -> String {
    let fragment = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if fragment.is_empty() {
        "unknown".to_string()
    } else {
        fragment
    }
}

#[cfg(test)]
#[derive(Debug)]
struct FakeRemote {
    snapshots: VecDeque<RemoteSnapshot>,
    refs: Vec<(String, String)>,
    reset_targets: Vec<String>,
    receipt_writes: usize,
    fail_ref: bool,
    fail_receipt: bool,
}

#[cfg(test)]
impl FakeRemote {
    fn new(snapshots: Vec<RemoteSnapshot>) -> Self {
        Self {
            snapshots: snapshots.into(),
            refs: Vec::new(),
            reset_targets: Vec::new(),
            receipt_writes: 0,
            fail_ref: false,
            fail_receipt: false,
        }
    }
}

#[cfg(test)]
#[async_trait]
impl RemoteGitBackend for FakeRemote {
    async fn snapshot(&mut self, _remote_dir: &str) -> Result<RemoteSnapshot, CleanupError> {
        self.snapshots
            .pop_front()
            .or_else(|| self.snapshots.back().cloned())
            .ok_or_else(|| CleanupError::Backend("no fake snapshot".into()))
    }

    async fn fetch_origin(&mut self, _remote_dir: &str) -> Result<(), CleanupError> {
        Ok(())
    }

    async fn create_ref(
        &mut self,
        _remote_dir: &str,
        ref_name: &str,
        sha: &str,
    ) -> Result<(), CleanupError> {
        if self.fail_ref {
            return Err(CleanupError::Backend("ref rejected".into()));
        }
        self.refs.push((ref_name.into(), sha.into()));
        Ok(())
    }

    async fn write_receipt(&mut self, receipt: &CleanupReceipt) -> Result<PathBuf, CleanupError> {
        if self.fail_receipt {
            return Err(CleanupError::Backend("disk full".into()));
        }
        self.receipt_writes += 1;
        Ok(receipt
            .receipt_path
            .clone()
            .unwrap_or_else(|| PathBuf::from(format!("receipt-{}.json", self.receipt_writes))))
    }

    async fn reset_hard(&mut self, _remote_dir: &str, target: &str) -> Result<(), CleanupError> {
        self.reset_targets.push(target.into());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(policy: CleanupPolicy) -> CleanupRequest {
        CleanupRequest {
            run_id: "run-one".into(),
            tab_id: Some(3),
            remote_host: "example-host".into(),
            remote_dir: "/srv/project".into(),
            policy,
            receipt_dir: PathBuf::from("receipts"),
        }
    }

    #[tokio::test]
    async fn block_policy_stops_clean_divergence() {
        let mut remote = FakeRemote::new(vec![RemoteSnapshot::clean("head-a", "origin-b")]);
        let error = cleanup_remote_checkout(&mut remote, request(CleanupPolicy::Block))
            .await
            .expect_err("blocked");
        assert!(matches!(error, CleanupError::DivergentBlocked { .. }));
        assert!(remote.refs.is_empty());
    }

    #[tokio::test]
    async fn preserve_reset_creates_ref_writes_receipt_and_resets() {
        let mut remote = FakeRemote::new(vec![
            RemoteSnapshot::clean("head-a", "origin-b"),
            RemoteSnapshot::clean("head-a", "origin-c"),
            RemoteSnapshot::clean("origin-c", "origin-c"),
        ]);
        let receipt = cleanup_remote_checkout(&mut remote, request(CleanupPolicy::PreserveReset))
            .await
            .expect("preserve reset");
        assert_eq!(receipt.outcome, CleanupOutcome::PreservedReset);
        assert_eq!(receipt.preserved_sha.as_deref(), Some("head-a"));
        assert_eq!(remote.refs.len(), 1);
        assert!(remote.refs[0]
            .0
            .starts_with("refs/heads/jailgun-preserved/run-one-"));
        assert_eq!(remote.reset_targets, vec!["origin-c"]);
        assert_eq!(remote.receipt_writes, 2);
        assert!(receipt
            .receipt_path
            .as_ref()
            .expect("receipt path")
            .starts_with("receipts/run-one"));
    }

    #[tokio::test]
    async fn dirty_remote_stops_before_preserve() {
        let mut remote = FakeRemote::new(vec![RemoteSnapshot::dirty(
            "head-a",
            "origin-b",
            " M src/lib.rs",
        )]);
        let error = cleanup_remote_checkout(&mut remote, request(CleanupPolicy::PreserveReset))
            .await
            .expect_err("dirty");
        assert!(matches!(error, CleanupError::DirtyRemote { .. }));
        assert!(remote.refs.is_empty());
    }

    #[tokio::test]
    async fn receipt_failure_stops_before_reset() {
        let mut remote = FakeRemote::new(vec![RemoteSnapshot::clean("head-a", "origin-b")]);
        remote.fail_receipt = true;
        let error = cleanup_remote_checkout(&mut remote, request(CleanupPolicy::PreserveReset))
            .await
            .expect_err("receipt failed");
        assert!(matches!(error, CleanupError::Receipt(_)));
        assert_eq!(remote.refs.len(), 1);
        assert!(remote.reset_targets.is_empty());
    }

    #[tokio::test]
    async fn ref_failure_stops_before_receipt_and_reset() {
        let mut remote = FakeRemote::new(vec![RemoteSnapshot::clean("head-a", "origin-b")]);
        remote.fail_ref = true;
        let error = cleanup_remote_checkout(&mut remote, request(CleanupPolicy::PreserveReset))
            .await
            .expect_err("ref failed");
        assert!(matches!(error, CleanupError::PreserveRef(_)));
        assert_eq!(remote.receipt_writes, 0);
        assert!(remote.reset_targets.is_empty());
    }
}

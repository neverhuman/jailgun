use std::path::PathBuf;

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

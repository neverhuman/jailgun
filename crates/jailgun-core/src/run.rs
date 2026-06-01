use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DeployQueueState {
    Idle,
    Waiting,
    Running,
    Blocked,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TabSnapshot {
    pub tab_id: u16,
    pub status: String,
    pub page_url: String,
    pub archive_sha256: Option<String>,
    pub download_latency_ms: Option<u64>,
    pub deploy_status: String,
    pub prompt_policy_decision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunSnapshot {
    pub run_id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub batch_tabs: u16,
    pub loop_count: u16,
    pub planned_tabs: u16,
    pub tabs: Vec<TabSnapshot>,
    pub deploy_queue: DeployQueueState,
    pub denied_github_prompts: u32,
    pub allowed_info_prompts: u32,
}

impl RunSnapshot {
    pub fn fixture() -> Self {
        Self {
            run_id: "fixture-run".into(),
            started_at: "2026-01-01T00:00:00Z".into(),
            finished_at: None,
            status: "running".into(),
            batch_tabs: 3,
            loop_count: 0,
            planned_tabs: 3,
            deploy_queue: DeployQueueState::Running,
            denied_github_prompts: 2,
            allowed_info_prompts: 1,
            tabs: vec![
                TabSnapshot {
                    tab_id: 1,
                    status: "downloaded".into(),
                    page_url: "https://chatgpt.com/c/example-one".into(),
                    archive_sha256: Some("abc123".into()),
                    download_latency_ms: Some(1200),
                    deploy_status: "validated".into(),
                    prompt_policy_decision: Some("deny".into()),
                },
                TabSnapshot {
                    tab_id: 2,
                    status: "remote-running".into(),
                    page_url: "https://chatgpt.com/c/example-two".into(),
                    archive_sha256: Some("def456".into()),
                    download_latency_ms: Some(1700),
                    deploy_status: "remote-job-launched".into(),
                    prompt_policy_decision: Some("allow-info".into()),
                },
            ],
        }
    }
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TarValidation {
    pub size_bytes: u64,
    pub entry_count: usize,
    pub files: Vec<String>,
    pub top_levels: Vec<String>,
    pub top_level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TarCandidate {
    pub index: usize,
    pub text: String,
    pub href: String,
    pub download: String,
    pub aria: String,
    pub title: String,
    pub base_score: i32,
    pub final_score: i32,
}

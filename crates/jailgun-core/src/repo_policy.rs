use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoStringFinding {
    pub path: PathBuf,
    pub pattern_label: String,
}

pub fn default_personal_patterns() -> Vec<(String, String)> {
    vec![
        ("blocked-host".into(), concat!("x", "babe2").into()),
        (
            "blocked-remote-dir".into(),
            concat!("/home/ubuntu/", "jekko").into(),
        ),
        (
            "blocked-repository".into(),
            concat!("neverhuman/", "jekko").into(),
        ),
        (
            "local-user-path".into(),
            concat!("/Users/", "bentaylor").into(),
        ),
        (
            "personal-email-prefix".into(),
            concat!("jepson", "@").into(),
        ),
        ("personal-domain".into(), concat!("veox", ".ai").into()),
    ]
}

pub fn scan_text_for_patterns(text: &str, patterns: &[(String, String)]) -> Vec<String> {
    patterns
        .iter()
        .filter(|(_, pattern)| text.contains(pattern))
        .map(|(label, _)| label.clone())
        .collect()
}

pub fn scan_file(path: impl AsRef<Path>) -> std::io::Result<Vec<RepoStringFinding>> {
    let path = path.as_ref();
    let text = fs::read_to_string(path)?;
    Ok(scan_text_for_patterns(&text, &default_personal_patterns())
        .into_iter()
        .map(|pattern_label| RepoStringFinding {
            path: path.to_path_buf(),
            pattern_label,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_split_personal_patterns_at_runtime() {
        let text = format!("deploy to {}", concat!("/home/ubuntu/", "jekko"));
        let findings = scan_text_for_patterns(&text, &default_personal_patterns());
        assert_eq!(findings, vec!["blocked-remote-dir"]);
    }
}

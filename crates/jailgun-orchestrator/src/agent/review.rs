use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result};
use jailgun_core::{
    JailgunAgentRunSummary, JailgunChangedFile, JailgunReviewPacket,
    JAILGUN_AGENT_INTERFACE_VERSION,
};

use crate::agent::timestamp_now;

pub fn build_review_packet(
    summary_json: &Path,
    repo: &Path,
    base: &str,
    head: &str,
    patch_bytes: usize,
) -> Result<JailgunReviewPacket> {
    let summary_text = fs::read_to_string(summary_json)
        .with_context(|| format!("reading {}", summary_json.display()))?;
    let summary: JailgunAgentRunSummary =
        serde_json::from_str(&summary_text).context("parsing run summary JSON")?;
    let base_sha = git_output(repo, &["rev-parse", base])?;
    let head_sha = git_output(repo, &["rev-parse", head])?;
    let base_sha = base_sha.trim().to_string();
    let head_sha = head_sha.trim().to_string();
    let diff_stat = git_output(
        repo,
        &[
            "diff",
            "--stat",
            "--find-renames",
            base_sha.as_str(),
            head_sha.as_str(),
        ],
    )?;
    let name_status_text = git_output(
        repo,
        &[
            "diff",
            "--name-status",
            "--find-renames",
            base_sha.as_str(),
            head_sha.as_str(),
        ],
    )?;
    let patch = cap_utf8(
        git_output(
            repo,
            &[
                "diff",
                "--no-ext-diff",
                "--find-renames",
                "--unified=80",
                base_sha.as_str(),
                head_sha.as_str(),
            ],
        )?,
        patch_bytes,
    );
    let name_status = parse_name_status(&name_status_text);
    let changed_tests = name_status
        .iter()
        .filter_map(|file| is_test_path(&file.path).then_some(file.path.clone()))
        .collect::<Vec<_>>();
    let mut source_metadata = BTreeMap::new();
    source_metadata.insert("repo_path".into(), repo.display().to_string());
    source_metadata.insert("summary_json".into(), summary_json.display().to_string());
    source_metadata.insert(
        "events_jsonl".into(),
        summary.events_jsonl.display().to_string(),
    );
    source_metadata.insert(
        "interface_version".into(),
        JAILGUN_AGENT_INTERFACE_VERSION.to_string(),
    );

    Ok(JailgunReviewPacket {
        version: JAILGUN_AGENT_INTERFACE_VERSION,
        generated_at: timestamp_now(),
        run_id: summary.run_id.clone(),
        prompt_ref: summary.prompt_ref.clone(),
        base_sha,
        head_sha,
        diff_stat,
        name_status,
        patch,
        changed_tests,
        artifacts: summary.artifacts.clone(),
        receipt_paths: summary.receipt_paths.clone(),
        summary,
        source_metadata,
    })
}

fn git_output(repo: &Path, args: &[&str]) -> Result<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .with_context(|| format!("running git {}", args.join(" ")))?;
    if !output.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(super) fn parse_name_status(text: &str) -> Vec<JailgunChangedFile> {
    text.lines()
        .filter_map(|line| {
            let parts = line.split('\t').collect::<Vec<_>>();
            let status = parts.first()?.to_string();
            if status.starts_with('R') || status.starts_with('C') {
                Some(JailgunChangedFile {
                    status,
                    old_path: parts.get(1).map(|value| (*value).to_string()),
                    path: parts.get(2)?.to_string(),
                })
            } else {
                Some(JailgunChangedFile {
                    status,
                    path: parts.get(1)?.to_string(),
                    old_path: None,
                })
            }
        })
        .collect()
}

pub(super) fn is_test_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.starts_with("tests/")
        || lower.contains("/tests/")
        || lower.contains("__tests__")
        || lower.ends_with("_test.rs")
        || lower.ends_with("_tests.rs")
        || lower.ends_with(".test.ts")
        || lower.ends_with(".test.tsx")
        || lower.ends_with(".spec.ts")
        || lower.ends_with(".spec.tsx")
        || lower.ends_with(".test.mjs")
}

pub(super) fn cap_utf8(mut text: String, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text;
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    text.truncate(boundary);
    text.push_str(&format!("\n[patch truncated at {max_bytes} bytes]\n"));
    text
}

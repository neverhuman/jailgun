use crate::model::{AcceptedTurn, Capture};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct BrowserCapture {
    #[serde(flatten)]
    pub turn: AcceptedTurn,
    pub markdown: String,
    pub complete: bool,
    pub error_code: Option<String>,
    #[serde(default)]
    pub retry_at_ms: Option<i64>,
}

impl BrowserCapture {
    /// Schema parsing never discards the full response. The storage service records
    /// missing/invalid summaries as a failed candidate, preserving its text artifact.
    pub fn into_capture(self, stage: &str) -> Capture {
        let documents = json_blocks(&self.markdown);
        let summary = if stage == "explore" {
            unique_field(&documents, "candidate_summary")
                .and_then(|value| serde_json::from_value(value).ok())
        } else {
            None
        };
        let evaluations = if stage == "compare" {
            unique_field(&documents, "evaluations")
                .and_then(|value| serde_json::from_value(value).ok())
        } else {
            None
        };
        Capture {
            markdown: self.markdown,
            summary,
            evaluations,
            conversation_id: self.turn.conversation_id,
            conversation_url: self.turn.conversation_url,
            observed_model: self.turn.observed_model,
            complete: self.complete,
            error_code: self.error_code,
        }
    }
}

fn unique_field(documents: &[serde_json::Value], field: &str) -> Option<serde_json::Value> {
    let mut fields = documents.iter().filter_map(|document| document.get(field));
    let first = fields.next()?.clone();
    if fields.next().is_some() {
        None
    } else {
        Some(first)
    }
}

fn json_blocks(markdown: &str) -> Vec<serde_json::Value> {
    let mut documents = Vec::new();
    let mut fence = 0;
    let mut json = false;
    let mut body = String::new();
    for line in markdown.lines() {
        let line = line.trim_start();
        let ticks = line.chars().take_while(|c| *c == '`').count();
        if fence == 0 && ticks >= 3 {
            fence = ticks;
            json = line[ticks..].trim().eq_ignore_ascii_case("json");
            body.clear();
        } else if fence > 0 && ticks >= fence && line[ticks..].trim().is_empty() {
            if json {
                if let Ok(document) = serde_json::from_str(&body) {
                    documents.push(document);
                }
            }
            fence = 0;
            json = false;
        } else if fence > 0 && json {
            body.push_str(line);
            body.push('\n');
        }
    }
    documents
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structured_data_must_be_complete_unique_and_inside_a_json_fence() {
        let markdown="text {\"candidate_summary\":1}\n```json\n{\"candidate_summary\":{\"proposal\":\"synthetic\"}}\n```\n";
        let documents = json_blocks(markdown);
        assert_eq!(documents.len(), 1);
        assert!(unique_field(&documents, "candidate_summary").is_some());
        assert!(unique_field(&json_blocks(&markdown.repeat(2)), "candidate_summary").is_none());
        assert!(json_blocks("```json\n{\"incomplete\":true}").is_empty());
        assert!(json_blocks("````text\n```json\n{}\n```\n````").is_empty());
    }
}

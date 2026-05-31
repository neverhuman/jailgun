//! Shared helpers used by `cleanup`, `shell`, `launcher`, and `deploy`.
//!
//! These are intentionally `pub(crate)` because they are implementation
//! details of the deploy crate, not part of its public API.

pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(crate) fn sanitize_ref_fragment(value: &str) -> String {
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

/// Cap a log tail to the last `max_lines` lines and at most `max_bytes` total
/// bytes. Always truncates from the **front** so the most recent output is
/// preserved.
pub(crate) fn truncate_log_tail(input: &str, max_lines: usize, max_bytes: usize) -> String {
    let lines: Vec<&str> = input.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    let mut tail = lines[start..].join("\n");
    if tail.len() > max_bytes {
        let cut = tail.len() - max_bytes;
        let mut idx = cut;
        while !tail.is_char_boundary(idx) && idx < tail.len() {
            idx += 1;
        }
        tail = tail[idx..].to_string();
    }
    tail
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        assert_eq!(shell_quote("simple"), "'simple'");
    }

    #[test]
    fn sanitize_strips_unsafe_characters() {
        assert_eq!(sanitize_ref_fragment("run/1@boom"), "run-1-boom");
        assert_eq!(sanitize_ref_fragment("///"), "unknown");
        assert_eq!(sanitize_ref_fragment("---hello---"), "hello");
    }

    #[test]
    fn truncate_keeps_tail_lines_under_byte_cap() {
        let text = (0..100)
            .map(|n| format!("line-{n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let tail = truncate_log_tail(&text, 5, 256);
        assert!(tail.contains("line-99"));
        assert!(!tail.contains("line-50"));
    }

    #[test]
    fn truncate_respects_byte_cap_when_lines_are_long() {
        let text = "x".repeat(10_000);
        let tail = truncate_log_tail(&text, 100, 200);
        assert_eq!(tail.len(), 200);
    }
}

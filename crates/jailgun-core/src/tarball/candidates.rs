use super::{TarCandidate, TarValidation};

pub fn derive_changed_file_paths(
    validation: &TarValidation,
    strip_components: usize,
) -> Vec<String> {
    validation
        .files
        .iter()
        .filter_map(|file| {
            let parts = file
                .split('/')
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>();
            let stripped = parts
                .iter()
                .skip(strip_components)
                .copied()
                .collect::<Vec<_>>()
                .join("/");
            (!stripped.is_empty()).then_some(stripped)
        })
        .collect()
}

pub fn rank_tar_candidates(candidates: &[TarCandidate], target_name: &str) -> Vec<TarCandidate> {
    let target = normalize_tar_name(target_name);
    let mut ranked = candidates
        .iter()
        .cloned()
        .map(|mut candidate| {
            let mut score = candidate.base_score;
            let haystack = [
                candidate.text.as_str(),
                candidate.href.as_str(),
                candidate.download.as_str(),
                candidate.aria.as_str(),
                candidate.title.as_str(),
            ]
            .join(" ");
            if !target.is_empty() {
                let normalized = normalize_tar_name(&haystack);
                if normalized == target {
                    score += 850;
                } else if normalized.contains(&target) || target.contains(&normalized) {
                    score += 250;
                }
            }
            if haystack.to_ascii_lowercase().contains(".tar.gz") {
                score += 150;
            }
            if haystack.to_ascii_lowercase().contains("download") {
                score += 60;
            }
            candidate.final_score = score;
            candidate
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .final_score
            .cmp(&left.final_score)
            .then(left.index.cmp(&right.index))
    });
    ranked
}

fn normalize_tar_name(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(".tar.gz")
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

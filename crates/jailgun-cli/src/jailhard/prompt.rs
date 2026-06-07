use super::*;

pub(super) fn hardening_prompt(
    manifest: &SourceManifest,
    target_count: Option<u16>,
    task_override: Option<&str>,
) -> String {
    let task = if let Some(task) = task_override {
        task.to_string()
    } else if let Some(count) = target_count {
        format!(
            "Generate novel expander source files for the selected Python expander catalog.\n\
Create and attach exactly one downloadable source.tar.gz artifact. Do not respond with a plan, progress narration, markdown, prose, diffs, placeholders, ellipses, or partial snippets.\n\
The artifact must contain exactly {count} new root-level .py files and nothing else. Do not include directories, nested paths, or existing filenames.\n\
Every returned file must be full source code only and implement one novel expander idea.\n\
Every file must define exactly one BaseExpander subclass. The subclass name must match the file stem, or the file must set STAGE_CLASS to that subclass.\n\
Do not use import statements. Assume the runtime injects globals used by the existing catalog, including np, pd, time, BaseExpander, Any, Optional, Union, List, Dict, Tuple, Callable, logging, math, and random.\n\
Implement fit, transform, fit_transform, get_params, and set_params consistently with the existing catalog style. Preserve input rows and append deterministic, named feature columns.\n\
Keep outputs source-only and compatible with the archive root.\n"
        )
    } else {
        "Do an aggressive bug, security, performance, refactor, and test review. Improve defects you can fix without breaking user-visible behavior.\n\
Add faster and broader tests where practical. Do not make behavior-breaking changes.\n"
            .to_string()
    };
    format!(
        "You are reviewing a source-only Jailgun hardening archive.\n\
Return exactly one .tar.gz artifact containing changed files rooted at the archive root. Do not wrap files in a project folder.\n\
The archive will be extracted over the invocation directory, so include only paths that should be overwritten.\n\
\n\
{task}\n\
\n\
Jankurai outline: Rust owns durable policy, config, tar validation, receipts, and run contracts. TypeScript owns browser/dashboard surfaces.\n\
Do not commit secrets, runtime state, real prompts, browser profiles, downloaded archives, logs, receipts, local override config, or private local paths.\n\
Keep edits scoped to the selected paths and preserve fakeable shell/browser/remote boundaries. The mapped proof lane must remain runnable.\n\
\n\
Selected target paths: {targets}\n\
Selected file count: {count}\n",
        targets = manifest.target_paths.join(", "),
        count = manifest.selected_files.len(),
    )
}

pub(super) fn read_task_file(path: &Path) -> Result<String> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("reading task file {}", path.display()))?;
    if text.trim().is_empty() {
        anyhow::bail!("task file is empty: {}", path.display());
    }
    Ok(text)
}

pub(super) fn parse_target_count(value: &str) -> std::result::Result<u16, String> {
    let count = value
        .parse::<u16>()
        .map_err(|_| "target-count must be a positive integer".to_string())?;
    if count == 0 {
        Err("target-count must be greater than zero".to_string())
    } else {
        Ok(count)
    }
}

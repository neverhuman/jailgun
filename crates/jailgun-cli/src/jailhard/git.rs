use super::*;

pub(super) fn ensure_clean_worktree(repo: &Path) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["status", "--porcelain=v1", "--untracked-files=normal"])
        .output()
        .context("checking git worktree status")?;
    if !output.status.success() {
        anyhow::bail!(
            "jailhard apply requires a git worktree: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    if !output.stdout.is_empty() {
        anyhow::bail!("jailhard apply requires a clean git worktree");
    }
    Ok(())
}

pub(super) fn git_diff_binary(repo: &Path) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["diff", "--binary"])
        .output()
        .context("running git diff --binary")?;
    if !output.status.success() {
        anyhow::bail!(
            "git diff --binary failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub(super) fn git_root(path: &Path) -> Result<Option<PathBuf>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("checking git repository root")?;
    if !output.status.success() {
        return Ok(None);
    }
    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if root.is_empty() {
        Ok(None)
    } else {
        Ok(Some(PathBuf::from(root)))
    }
}

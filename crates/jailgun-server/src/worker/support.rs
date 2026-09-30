use super::*;

pub(super) fn default_model() -> String {
    "gpt-6-astra".into()
}

pub(super) fn default_timeout_seconds() -> u64 {
    DEFAULT_TIMEOUT_SECONDS
}

pub(super) fn default_chunk_limit() -> u32 {
    64 * 1024
}

pub(super) fn default_true() -> bool {
    true
}

pub(super) fn account_counts<'a>(
    accounts: impl Iterator<Item = &'a WorkerAccount>,
) -> WorkerAccountCounts {
    let mut counts = WorkerAccountCounts::default();
    for account in accounts {
        counts.total += 1;
        match account.status {
            WorkerAccountStatus::Ready => counts.ready += 1,
            WorkerAccountStatus::LoginRequired => counts.login_required += 1,
            WorkerAccountStatus::Unavailable => counts.unavailable += 1,
        }
    }
    counts
}

pub(super) fn model_aliases() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("astra".into(), "gpt-6-astra".into()),
        ("sol".into(), "gpt-5.6-sol".into()),
    ])
}

pub(super) fn normalize_model(model: &str) -> Result<String> {
    let value = model.trim().to_lowercase();
    if let Some(model) = model_aliases().get(&value) {
        return Ok(model.clone());
    }
    if value.is_empty()
        || value.len() > 80
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
    {
        return Err(action(
            "invalid-model",
            "Model must be an alias such as astra/sol or a safe model identifier.",
            "Call jailgun.worker.info for aliases and supported efforts.",
        ));
    }
    Ok(value)
}

pub(super) fn validate_job(request: &JobSubmit) -> Result<()> {
    let prompt_chars = request.prompt.chars().count();
    if prompt_chars == 0 || prompt_chars > MAX_PROMPT_CHARS {
        return Err(action(
            "invalid-prompt",
            format!("prompt must contain 1–{MAX_PROMPT_CHARS} characters."),
            "Send a bounded task prompt.",
        ));
    }
    if request.idempotency_key.is_empty() || request.idempotency_key.len() > 128 {
        return Err(action(
            "invalid-idempotency-key",
            "idempotency_key must contain 1–128 bytes.",
            "Use a stable unique key for this logical submission.",
        ));
    }
    if request.timeout_seconds == 0 || request.timeout_seconds > MAX_TIMEOUT_SECONDS {
        return Err(action(
            "invalid-timeout",
            format!("timeout_seconds must be between 1 and {MAX_TIMEOUT_SECONDS}."),
            "Choose a bounded timeout.",
        ));
    }
    if let Some(tab_id) = &request.tab_id {
        validate_id(tab_id, "tab")?;
    }
    Ok(())
}

pub(super) fn validate_object_request(request: &ObjectPut) -> Result<()> {
    if request.name.is_empty()
        || request.name.len() > 160
        || !request.name.ends_with(".tar.gz")
        || Path::new(&request.name)
            .file_name()
            .and_then(|v| v.to_str())
            != Some(&request.name)
    {
        return Err(action(
            "invalid-object-name",
            "Object names must be safe .tar.gz basenames.",
            "Use a name such as source.tar.gz.",
        ));
    }
    if request.total_bytes == 0 || request.total_bytes > MAX_OBJECT_BYTES {
        return Err(action(
            "invalid-object-size",
            format!("total_bytes must be between 1 and {MAX_OBJECT_BYTES}."),
            "Reduce the archive size before uploading.",
        ));
    }
    if request.sha256.len() != 64
        || !request
            .sha256
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(action(
            "invalid-sha256",
            "sha256 must be a 64-character hexadecimal digest.",
            "Hash the complete tar.gz bytes and retry.",
        ));
    }
    if request.offset > request.total_bytes {
        return Err(action(
            "invalid-upload-offset",
            "offset exceeds total_bytes.",
            "Restart at offset zero or use the returned next_offset.",
        ));
    }
    Ok(())
}

pub(super) fn validate_id(value: &str, kind: &'static str) -> Result<()> {
    if value.is_empty()
        || value.len() > 96
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
    {
        return Err(action(
            "invalid-id",
            format!("{kind}_id contains unsafe characters."),
            "Use 1–96 ASCII letters, digits, hyphens, or underscores.",
        ));
    }
    Ok(())
}

pub(super) fn is_terminal(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled" | "timed-out")
}

pub(super) fn action(
    code: &'static str,
    message: impl Into<String>,
    next_action: &'static str,
) -> Error {
    Error::action(code, message, next_action)
}

pub(super) fn not_found(kind: &'static str) -> Error {
    action(
        "not-found",
        format!("The requested {kind} does not exist."),
        "List the available worker resources and retry with an existing ID.",
    )
}

pub(super) fn io_failure(error: std::io::Error) -> ExecutionFailure {
    ExecutionFailure {
        kind: ExecutionFailureKind::Failed,
        message: format!("executor-io-failed: {error}"),
    }
}

pub(super) fn error_detail(path: &Path, mode: ErrorReporting) -> String {
    if mode == ErrorReporting::Off {
        return String::new();
    }
    let Ok(mut bytes) = std::fs::read(path) else {
        return String::new();
    };
    let limit = match mode {
        ErrorReporting::Off => 0,
        ErrorReporting::Summary => 4 * 1024,
        ErrorReporting::Detailed => 64 * 1024,
    };
    if bytes.len() > limit {
        bytes = bytes.split_off(bytes.len() - limit);
    }
    String::from_utf8_lossy(&bytes).trim().to_string()
}

pub(super) fn stderr_indicates_auth_failure(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let text = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
    [
        "not logged in",
        "login required",
        "authentication required",
        "unauthorized",
        "status 401",
        "http 401",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

pub(super) fn restrict_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub(super) fn persist_object(root: &Path, object: &WorkerObject) -> Result<()> {
    let path = root
        .join("objects")
        .join(format!("{}.json", object.object_id));
    let staging_path = path.with_extension("json.tmp");
    std::fs::write(&staging_path, serde_json::to_vec_pretty(object)?)?;
    std::fs::rename(staging_path, path)?;
    Ok(())
}

pub(super) fn persist_account(root: &Path, account: &WorkerAccount) -> Result<()> {
    let path = root
        .join("accounts")
        .join(&account.account_id)
        .join("account.json");
    let staging_path = path.with_extension("json.tmp");
    std::fs::write(&staging_path, serde_json::to_vec_pretty(account)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staging_path, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(staging_path, path)?;
    Ok(())
}

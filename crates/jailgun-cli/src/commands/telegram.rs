use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use jailgun_notify::{
    build_commit_message, collect_commit_summary, read_chat_id_cache, send_telegram_message,
    write_chat_id_cache, CommitSummary, TelegramConfig,
};

pub(super) async fn telegram_send(
    token_file: PathBuf,
    chat_id_cache: PathBuf,
    chat_id: Option<String>,
    message: String,
) -> Result<()> {
    let mut config = TelegramConfig::from_token_file(&token_file)
        .with_context(|| format!("loading {}", token_file.display()))?;
    if let Some(chat_id) = chat_id {
        config.chat_id = Some(chat_id);
    }
    if config.chat_id.is_none() {
        config.chat_id = read_chat_id_cache(&chat_id_cache)?;
    }
    let sent_chat_id = send_telegram_message(&config, &message).await?;
    write_chat_id_cache(&chat_id_cache, &sent_chat_id)?;
    println!(
        "{}",
        serde_json::json!({
            "status": "sent",
            "chat_id": sent_chat_id,
        })
    );
    Ok(())
}

pub(super) async fn notify_commit(
    token_file: PathBuf,
    chat_id_cache: PathBuf,
    chat_id: Option<String>,
    repo: PathBuf,
    revision: String,
) -> Result<()> {
    let mut config = TelegramConfig::from_token_file(&token_file)
        .with_context(|| format!("loading {}", token_file.display()))?;
    if let Some(chat_id) = chat_id {
        config.chat_id = Some(chat_id);
    }
    if config.chat_id.is_none() {
        config.chat_id = read_chat_id_cache(&chat_id_cache)?;
    }
    let summary = collect_commit_summary(&repo, &revision)
        .with_context(|| format!("collecting commit summary for {revision}"))?;
    let message = build_commit_message(&summary);
    let sent_chat_id = send_telegram_message(&config, &message).await?;
    write_chat_id_cache(&chat_id_cache, &sent_chat_id)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&notify_result(sent_chat_id, summary))?
    );
    Ok(())
}

fn notify_result(chat_id: String, summary: CommitSummary) -> serde_json::Value {
    serde_json::json!({
        "status": "sent",
        "chat_id": chat_id,
        "commit": summary.short_hash,
        "subject": summary.subject,
        "files": summary.files,
    })
}

pub(super) fn validate_telegram_notify(token_file: &Path, chat_id_cache: &Path) -> Result<()> {
    let mut config = TelegramConfig::from_token_file(token_file)
        .with_context(|| format!("loading {}", token_file.display()))?;
    if config.chat_id.is_none() {
        config.chat_id = read_chat_id_cache(chat_id_cache)?;
    }
    if config
        .chat_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        anyhow::bail!(
            "--notify-telegram requires a chat id in {} or {}",
            token_file.display(),
            chat_id_cache.display()
        );
    }
    Ok(())
}

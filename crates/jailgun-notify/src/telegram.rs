use std::{fs, path::Path};

use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TelegramError {
    #[error("could not read Telegram token file {path}: {source}")]
    ReadToken {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("Telegram bot token is missing")]
    MissingToken,
    #[error("Telegram chat id is missing and no chat could be discovered from getUpdates")]
    MissingChatId,
    #[error("Telegram API request failed: {0}")]
    Request(reqwest::Error),
    #[error("Telegram API returned ok=false: {0}")]
    Api(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub chat_id: Option<String>,
}

impl TelegramConfig {
    pub fn from_token_file(path: impl AsRef<Path>) -> Result<Self, TelegramError> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|source| TelegramError::ReadToken {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_token_text(&text)
    }

    pub fn from_token_text(text: &str) -> Result<Self, TelegramError> {
        let mut bot_token = String::new();
        let mut chat_id = None;
        for raw_line in text.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let line = line.strip_prefix("export ").unwrap_or(line).trim();
            if let Some((key, value)) = line.split_once('=') {
                let value = unquote(value.trim());
                match key.trim() {
                    "BOT_TOKEN" | "TELEGRAM_BOT_TOKEN" | "JAILGUN_TELEGRAM_BOT_TOKEN" => {
                        bot_token = value;
                    }
                    "CHAT_ID" | "TELEGRAM_CHAT_ID" | "JAILGUN_TELEGRAM_CHAT_ID"
                        if !value.is_empty() =>
                    {
                        chat_id = Some(value);
                    }
                    _ => {}
                }
            } else if looks_like_bot_token(line) && bot_token.is_empty() {
                bot_token = line.to_string();
            } else if looks_like_chat_id(line) && chat_id.is_none() {
                chat_id = Some(line.to_string());
            }
        }
        if bot_token.is_empty() {
            return Err(TelegramError::MissingToken);
        }
        Ok(Self { bot_token, chat_id })
    }
}

pub async fn send_telegram_message(
    config: &TelegramConfig,
    message: &str,
) -> Result<String, TelegramError> {
    let client = Client::new();
    let chat_id = match &config.chat_id {
        Some(chat_id) => chat_id.clone(),
        None => discover_chat_id(&client, &config.bot_token).await?,
    };
    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage",
        config.bot_token
    );
    let response = client
        .post(url)
        .form(&[
            ("chat_id", chat_id.as_str()),
            ("text", message),
            ("disable_web_page_preview", "true"),
        ])
        .send()
        .await
        .map_err(sanitize_reqwest_error)?
        .error_for_status()
        .map_err(sanitize_reqwest_error)?
        .json::<SendMessageResponse>()
        .await
        .map_err(sanitize_reqwest_error)?;
    if !response.ok {
        return Err(TelegramError::Api(
            response
                .description
                .unwrap_or_else(|| "sendMessage failed".into()),
        ));
    }
    Ok(chat_id)
}

async fn discover_chat_id(client: &Client, bot_token: &str) -> Result<String, TelegramError> {
    let url = format!("https://api.telegram.org/bot{bot_token}/getUpdates");
    let response = client
        .get(url)
        .query(&[("limit", "10"), ("timeout", "0")])
        .send()
        .await
        .map_err(sanitize_reqwest_error)?
        .error_for_status()
        .map_err(sanitize_reqwest_error)?
        .json::<GetUpdatesResponse>()
        .await
        .map_err(sanitize_reqwest_error)?;
    if !response.ok {
        return Err(TelegramError::Api(
            response
                .description
                .unwrap_or_else(|| "getUpdates failed".into()),
        ));
    }
    response
        .result
        .into_iter()
        .rev()
        .find_map(|update| update.message.map(|message| message.chat.id.to_string()))
        .ok_or(TelegramError::MissingChatId)
}

#[derive(Debug, Deserialize)]
struct GetUpdatesResponse {
    ok: bool,
    #[serde(default)]
    result: Vec<Update>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Update {
    message: Option<Message>,
}

#[derive(Debug, Deserialize)]
struct Message {
    chat: Chat,
}

#[derive(Debug, Deserialize)]
struct Chat {
    id: i64,
}

#[derive(Debug, Deserialize)]
struct SendMessageResponse {
    ok: bool,
    description: Option<String>,
}

fn looks_like_bot_token(value: &str) -> bool {
    let Some((left, right)) = value.split_once(':') else {
        return false;
    };
    left.chars().all(|ch| ch.is_ascii_digit()) && right.len() > 20
}

fn looks_like_chat_id(value: &str) -> bool {
    value
        .strip_prefix('-')
        .unwrap_or(value)
        .chars()
        .all(|ch| ch.is_ascii_digit())
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

fn sanitize_reqwest_error(error: reqwest::Error) -> TelegramError {
    TelegramError::Request(error.without_url())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_raw_bot_token() {
        let config = TelegramConfig::from_token_text("123456:abcdefghijklmnopqrstuvwxyzABCDE")
            .expect("token");
        assert!(config.bot_token.starts_with("123456:"));
        assert_eq!(config.chat_id, None);
    }

    #[test]
    fn parses_env_style_token_and_chat_id() {
        let config = TelegramConfig::from_token_text(
            "TELEGRAM_BOT_TOKEN='123456:abcdefghijklmnopqrstuvwxyzABCDE'\nTELEGRAM_CHAT_ID=-10042\n",
        )
        .expect("token");
        assert_eq!(config.chat_id.as_deref(), Some("-10042"));
    }
}

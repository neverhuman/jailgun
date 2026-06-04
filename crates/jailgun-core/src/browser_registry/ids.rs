use sha2::{Digest, Sha256};

use super::BrowserRegistryError;

pub fn validate_account_id(id: &str) -> Result<(), BrowserRegistryError> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(BrowserRegistryError::EmptyAccountId);
    }
    if trimmed == "." || trimmed == ".." {
        return Err(BrowserRegistryError::InvalidAccountId(id.to_string()));
    }
    if trimmed.len() > 64 {
        return Err(BrowserRegistryError::InvalidAccountId(id.to_string()));
    }
    if !trimmed
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(BrowserRegistryError::InvalidAccountId(id.to_string()));
    }
    Ok(())
}

pub fn default_account_id(email_hint: &str) -> String {
    let normalized = email_hint.trim().to_ascii_lowercase();
    let digest = Sha256::digest(normalized.as_bytes());
    format!("acct-{}", hex_prefix(&digest, 8))
}

fn hex_prefix(bytes: &[u8], chars: usize) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(chars);
    for byte in bytes {
        if out.len() >= chars {
            break;
        }
        out.push(HEX[(byte >> 4) as usize] as char);
        if out.len() >= chars {
            break;
        }
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

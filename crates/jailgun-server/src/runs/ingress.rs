use std::collections::HashSet;

use jailgun_core::JailgunAgentRunRequest;
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunIngress {
    Rest,
    Mcp,
}

pub(super) fn parse_agent_run_request(
    mut body: Value,
    ingress: RunIngress,
) -> Result<JailgunAgentRunRequest, String> {
    match body.get("version").and_then(Value::as_u64) {
        Some(version) if version == jailgun_core::JAILGUN_AGENT_INTERFACE_VERSION as u64 => {}
        Some(version) => {
            return Err(format!(
                "unsupported Jailgun agent interface version {}; expected {}",
                version,
                jailgun_core::JAILGUN_AGENT_INTERFACE_VERSION
            ));
        }
        None => {}
    }
    normalize_agent_run_accounts(&mut body, ingress)?;
    serde_json::from_value(body).map_err(|error| format!("parsing agent request JSON: {error}"))
}

fn normalize_agent_run_accounts(body: &mut Value, ingress: RunIngress) -> Result<(), String> {
    let Some(object) = body.as_object_mut() else {
        return Err("agent run request must be a JSON object".into());
    };

    let canonical = object
        .get("browser")
        .and_then(|browser| browser.get("account_ids"))
        .map(|value| parse_account_ids(value, "browser.account_ids"))
        .transpose()?;
    let rest_alias = object
        .get("account_ids")
        .map(|value| parse_account_ids(value, "account_ids"))
        .transpose()?;
    let mcp_alias = object
        .get("account")
        .map(|value| parse_account_alias(value, "account"))
        .transpose()?;

    let mut selected = canonical.clone();
    for alias in [rest_alias, mcp_alias].into_iter().flatten() {
        if let Some(canonical) = selected.as_ref() {
            if canonical != &alias {
                return Err(format!(
                    "conflicting browser account routing aliases for {:?}",
                    ingress
                ));
            }
        } else {
            selected = Some(alias);
        }
    }

    if let Some(account_ids) = selected {
        ensure_unique_account_ids(&account_ids)?;
        let browser = object
            .entry("browser")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| "browser must be a JSON object".to_string())?;
        browser.insert("account_ids".into(), json!(account_ids));
    }
    object.remove("account_ids");
    object.remove("account");
    Ok(())
}

fn parse_account_ids(value: &Value, field: &str) -> Result<Vec<String>, String> {
    let Some(values) = value.as_array() else {
        return Err(format!("{field} must be an array of browser account ids"));
    };
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("{field}[{index}] must be a string"))
        })
        .collect()
}

fn parse_account_alias(value: &Value, field: &str) -> Result<Vec<String>, String> {
    value
        .as_str()
        .map(|account| vec![account.to_string()])
        .ok_or_else(|| format!("{field} must be a browser account id string"))
}

fn ensure_unique_account_ids(account_ids: &[String]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for account_id in account_ids {
        if !seen.insert(account_id) {
            return Err(format!(
                "duplicate browser account id requested: {account_id}"
            ));
        }
    }
    Ok(())
}

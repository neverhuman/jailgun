pub(crate) mod client;
mod connect;
mod doctor;
pub mod error;
mod exports;
mod onboarding;
mod operations;
pub mod options;
pub mod service;
pub mod startup;
mod startup_paths;

pub(crate) use connect::configured_client;

use anyhow::Result;
use jailgun_workflow::model::{FinalResult, Run};
use options::{AccountsCommand, ConnectionOptions, RunsCommand};
use serde_json::Value;

pub async fn setup(options: ConnectionOptions, no_open: bool, json: bool) -> Result<bool> {
    let client = client::Client::connect(&options).await?;
    let result = onboarding::dashboard(&client, no_open).await?;
    if json {
        operations::print_value(&result, true)?;
    } else {
        println!(
            "Open {}\nThe pairing link expires in five minutes and works once.",
            result["dashboard_url"]
                .as_str()
                .unwrap_or("the local dashboard")
        );
    }
    Ok(true)
}
pub async fn doctor(options: ConnectionOptions, json: bool) -> Result<bool> {
    let report = doctor::inspect(&options).await?;
    operations::print_value(&report, json)?;
    Ok(report["status"] == "ready")
}
pub async fn brainstorm(options: options::BrainstormOptions, json: bool) -> Result<bool> {
    operations::brainstorm(options, json).await?;
    Ok(true)
}
pub async fn accounts(
    options: ConnectionOptions,
    command: AccountsCommand,
    json: bool,
) -> Result<bool> {
    let client = client::Client::connect(&options).await?;
    let result = match command {
        AccountsCommand::List => client.get::<Value>("/api/accounts").await?,
        AccountsCommand::Connect { email, id, no_open } => {
            if let Some(email) = email {
                let _: Value = client
                    .post(
                        "/api/accounts/connect",
                        &jailgun_workflow::model::ConnectAccount { email, id },
                    )
                    .await?;
            }
            onboarding::dashboard(&client, no_open).await?
        }
        AccountsCommand::Reconnect {
            account_id,
            no_open,
        } => {
            jailgun_core::validate_account_id(&account_id).map_err(|_| {
                error::failure(
                    "invalid-account-id",
                    "The account ID is invalid.",
                    "Use an ID from accounts list.",
                )
            })?;
            let _: Value = client
                .post(
                    &format!("/api/accounts/{account_id}/reconnect"),
                    &operations::empty(),
                )
                .await?;
            onboarding::dashboard(&client, no_open).await?
        }
    };
    operations::print_value(&result, json)?;
    Ok(true)
}
pub async fn runs(options: ConnectionOptions, command: RunsCommand, json: bool) -> Result<bool> {
    let client = client::Client::connect(&options).await?;
    let result = match command {
        RunsCommand::List => client.get::<Value>("/api/runs?kind=concept").await?,
        RunsCommand::Show { run_id } => {
            client.get::<Value>(&operations::run_path(&run_id)?).await?
        }
        RunsCommand::Result { run_id } => {
            let result: FinalResult = client
                .get(&format!("{}/result", operations::run_path(&run_id)?))
                .await?;
            operations::print_result(&client, &result, json).await?;
            return Ok(true);
        }
        RunsCommand::Pause { run_id } => {
            client
                .post::<Value>(
                    &format!("{}/pause", operations::run_path(&run_id)?),
                    &operations::empty(),
                )
                .await?
        }
        RunsCommand::Cancel { run_id } => {
            client
                .post::<Value>(
                    &format!("{}/cancel", operations::run_path(&run_id)?),
                    &operations::empty(),
                )
                .await?
        }
        RunsCommand::Resume {
            run_id,
            allow_incomplete,
        } => {
            client
                .post::<Value>(
                    &format!("{}/resume", operations::run_path(&run_id)?),
                    &jailgun_workflow::model::ResumeRequest { allow_incomplete },
                )
                .await?
        }
        RunsCommand::Export { run_id, out } => {
            let run: Run = client.get(&operations::run_path(&run_id)?).await?;
            exports::export(&client, run, &out).await?
        }
    };
    operations::print_value(&result, json)?;
    Ok(true)
}

#[cfg(test)]
mod tests;

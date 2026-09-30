//! The UI proof controls synthetic login through private stdin, never an HTTP route.
use anyhow::{Context, Result};
use jailgun_orchestrator::concept::AccountSupervisor;
use jailgun_workflow::Store;
use std::{io::Write, time::Duration};
use tokio::io::AsyncBufReadExt;

pub(super) async fn drive(
    url: &str,
    pairing: &str,
    service: &AccountSupervisor,
    store: &Store,
) -> Result<()> {
    println!("{}", serde_json::json!({"url":url,"pairing":pairing}));
    std::io::stdout().flush()?;
    let mut commands = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    let mut tick = tokio::time::interval(Duration::from_millis(25));
    let deadline = tokio::time::sleep(Duration::from_secs(180));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            command = commands.next_line() => match command?.as_deref() {
                Some("login") => {
                    let accounts = store.account_metadata().await?;
                    let account = accounts.first().context("connect an account in the dashboard first")?;
                    service.fixture_login(&account.id).await?;
                },
                Some("stop") | None => return Ok(()),
                Some("expire-login") => {
                    let accounts = store.account_metadata().await?;
                    let account = accounts.first().context("connect an account first")?;
                    store.login_state(account.id.clone(), "waiting-for-user".into(), None, Some(1)).await?;
                },
                _ => anyhow::bail!("unknown dashboard proof command"),
            },
            _ = tick.tick() => service.tick().await?,
            _ = &mut deadline => anyhow::bail!("dashboard proof timed out"),
        }
    }
}

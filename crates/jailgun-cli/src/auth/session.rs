use std::{
    collections::BTreeMap,
    io::{self, Write},
    time::Duration,
};

use anyhow::{Context, Result};
use jailgun_core::BrowserAccount;
use jailgun_orchestrator::bridge::{
    spawn_bridge, AuthBeginPayload, AuthSubmitCodePayload, BridgeCommand, BridgeSpawnConfig,
    HelloPayload, ShutdownPayload, PROTOCOL_VERSION,
};

use super::{
    bridge::{account_bridge_env, send_bridge_command},
    events::{next_auth_event, wait_for_bridge_ready, AuthEvent},
};

pub(super) async fn setup_one_account(
    account: &BrowserAccount,
    bridge_cmd: &[String],
    bridge_env: &BTreeMap<String, String>,
    prefer_email_code: bool,
    code_stdin: bool,
    status_watch: bool,
) -> Result<()> {
    let run_id = format!("auth-{}", account.id);
    let mut bridge = spawn_bridge(BridgeSpawnConfig {
        command: bridge_cmd.to_vec(),
        env: account_bridge_env(account, bridge_env),
    })
    .await?;

    send_bridge_command(
        &bridge,
        &run_id,
        BridgeCommand::Hello(HelloPayload {
            orchestrator_version: env!("CARGO_PKG_VERSION").into(),
            protocol_version: PROTOCOL_VERSION,
            capabilities: vec!["auth-control-plane".into()],
        }),
    )
    .await?;
    wait_for_bridge_ready(&mut bridge, status_watch).await?;

    send_bridge_command(
        &bridge,
        &run_id,
        BridgeCommand::AuthBegin(AuthBeginPayload {
            chat_url: "https://chatgpt.com/".into(),
            email_hint: account.email_hint.clone(),
            prefer_email_code,
            profile_dir: Some(account.profile_dir.display().to_string()),
        }),
    )
    .await?;

    let mut code_requested = false;
    loop {
        match next_auth_event(&mut bridge, status_watch).await? {
            AuthEvent::Complete => break,
            AuthEvent::CodeRequested => {
                code_requested = true;
                let code = read_code(&account.email_hint, code_stdin)?;
                send_bridge_command(
                    &bridge,
                    &run_id,
                    BridgeCommand::AuthSubmitCode(AuthSubmitCodePayload {
                        code,
                        profile_dir: Some(account.profile_dir.display().to_string()),
                    }),
                )
                .await?;
            }
            AuthEvent::ManualRequired(reason) => {
                anyhow::bail!("manual-browser-required: {reason}");
            }
            AuthEvent::Failed(reason) => anyhow::bail!("{reason}"),
        }
    }

    if !code_requested {
        eprintln!("account {} already had a valid ChatGPT session", account.id);
    }
    send_bridge_command(
        &bridge,
        &run_id,
        BridgeCommand::Shutdown(ShutdownPayload {
            drain_timeout_ms: 1_000,
        }),
    )
    .await
    .ok();
    let _ = tokio::time::timeout(Duration::from_secs(10), bridge.child.wait()).await;
    Ok(())
}

fn read_code(email_hint: &str, code_stdin: bool) -> Result<String> {
    if code_stdin {
        eprintln!("waiting for verification code on stdin for {email_hint}");
    } else {
        eprint!("Enter email verification code for {email_hint}: ");
        io::stderr().flush().ok();
    }
    let mut code = String::new();
    io::stdin()
        .read_line(&mut code)
        .context("reading verification code from stdin")?;
    let code = code.trim().to_string();
    if code.is_empty() {
        anyhow::bail!("verification code cannot be empty");
    }
    Ok(code)
}

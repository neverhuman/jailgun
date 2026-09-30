use std::path::PathBuf;

use anyhow::{Context, Result};
use jailgun_core::JailgunConfig;
use jailgun_server::{api_router, router_with_static, AppState};

use crate::{cli::FixtureKind, commands::telegram::validate_telegram_notify};

#[allow(clippy::too_many_arguments)]
pub(super) async fn serve(
    config: PathBuf,
    addr: std::net::SocketAddr,
    dashboard_dist: Option<PathBuf>,
    live: bool,
    ingest_token: Option<String>,
    notify_telegram: bool,
    telegram_token_file: PathBuf,
    telegram_chat_id_cache: PathBuf,
) -> Result<()> {
    let config_path = config.clone();
    let config = JailgunConfig::from_toml_path(&config)
        .with_context(|| format!("loading {}", config.display()))?;
    let receipt_dir = PathBuf::from(&config.paths.artifacts_dir).join("receipts");
    if !addr.ip().is_loopback() {
        anyhow::bail!("loopback-required: use an SSH local forward to access Jailgun remotely");
    }
    let registry = jailgun_core::BrowserProfileRegistry::default_path_from_env(
        &config.browser.profile_registry_env,
    );
    let runtime = registry
        .parent()
        .context("browser registry has no runtime directory")?;
    let installation = jailgun_core::installation::Installation::acquire(runtime)?;
    let token = match ingest_token {
        Some(token) => {
            jailgun_core::installation::validate_operator_token(&token)
                .map_err(anyhow::Error::msg)?;
            token
        }
        None => installation.operator_token()?,
    };
    let state = if live {
        let (state, rx) = AppState::live(config, receipt_dir, 1024);
        if notify_telegram {
            validate_telegram_notify(&telegram_token_file, &telegram_chat_id_cache)?;
            tokio::spawn(jailgun_notify::run_telegram_subscriber(
                rx,
                telegram_token_file,
                telegram_chat_id_cache,
            ));
        }
        state
            .with_ingest_token(Some(token.clone()))
            .with_config_path(Some(config_path))
    } else {
        if notify_telegram {
            anyhow::bail!("--notify-telegram requires --live");
        }
        AppState::fixture(config).with_ingest_token(Some(token))
    };
    let router = match dashboard_dist {
        Some(dir) => router_with_static(state, dir),
        None => api_router(state),
    };
    println!("listening on http://{addr} (live={live} notify_telegram={notify_telegram})");
    println!("Advanced dashboard: http://{addr}/?advanced=1");
    println!(
        "Request a pairing code from POST /api/session/pairing-code with the operator credential."
    );
    jailgun_server::serve(addr, router).await?;
    Ok(())
}

pub(super) async fn fixture(kind: FixtureKind) -> Result<()> {
    let config = JailgunConfig::default();
    let state = AppState::fixture(config);
    match kind {
        FixtureKind::Runs => println!(
            "{}",
            serde_json::to_string_pretty(&state.runs.read().await.clone())?
        ),
        FixtureKind::Config => println!(
            "{}",
            serde_json::to_string_pretty(&state.config.redacted_for_display())?
        ),
    }
    Ok(())
}

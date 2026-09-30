use anyhow::{Context, Result};
use jailgun_core::{installation::Installation, BrowserLeaseManager, JailgunConfig};
use jailgun_orchestrator::concept::{AccountSupervisor, BrowserRuntime};
use jailgun_server::{router_with_static, AppState, DaemonControl};
use jailgun_workflow::Store;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug, Default, clap::Args)]
pub struct ConceptDaemonOptions {
    /// Private runtime root on a local filesystem (defaults to ~/.jailgun).
    #[arg(long, global = true, env = "JAILGUN_RUNTIME")]
    pub runtime: Option<PathBuf>,
    /// Runtime assets root; installed default is ../lib/jailgun relative to the executable.
    #[arg(long, global = true, env = "JAILGUN_ASSETS")]
    pub assets: Option<PathBuf>,
    /// Explicit Node executable for development; installed default is assets/node/bin/node.
    #[arg(long, global = true)]
    pub node: Option<PathBuf>,
    #[arg(long, global = true)]
    pub chrome: Option<PathBuf>,
    #[arg(long, global = true, conflicts_with = "server_browser")]
    pub headless: bool,
    /// Linux: manage a private display and open login through the paired dashboard.
    #[arg(long, global = true)]
    pub server_browser: bool,
}

pub async fn serve(options: ConceptDaemonOptions, addr: std::net::SocketAddr) -> Result<()> {
    if !addr.ip().is_loopback() {
        anyhow::bail!("loopback-required: use an SSH local forward");
    }
    let runtime = match options.runtime {
        Some(root) => root,
        None => std::env::var_os("HOME")
            .map(PathBuf::from)
            .context("HOME is required or set --runtime")?
            .join(".jailgun"),
    };
    let installation = Installation::acquire(&runtime)?;
    let runtime = runtime.canonicalize()?;
    let assets = match options.assets {
        Some(path) => path
            .canonicalize()
            .context("runtime-assets-missing: inspect --assets")?,
        None => jailgun_core::runtime_assets::root()?,
    };
    let node = options.node.unwrap_or_else(|| assets.join("node/bin/node"));
    let bridge = assets.join("apps/chrome-bridge/bin/concept-bridge.mjs");
    let dashboard = assets.join("apps/dashboard/dist");
    if !node.is_file() || !bridge.is_file() || !dashboard.join("index.html").is_file() {
        anyhow::bail!("runtime-assets-missing: install a complete Jailgun bundle or set --assets and --node for development");
    }
    let listener = tokio::net::TcpListener::bind(addr).await.context(
        "daemon-address-busy: connect to the existing daemon or choose a different port",
    )?;
    let store = Store::open(&runtime)?;
    let registry = runtime.join("browser-profiles.json");
    if registry.exists() {
        BrowserLeaseManager::new(&registry).claim_workflow_profiles(&runtime)?;
        store.import_registry(registry, now()).await?;
    }
    let supervisor = AccountSupervisor::new(
        store.clone(),
        BrowserRuntime {
            node,
            bridge,
            chrome: options.chrome,
            headless: options.headless,
            server_browser: options.server_browser,
            provider_url: "https://chatgpt.com".into(),
            environment: BTreeMap::new(),
        },
    )
    .await?;
    supervisor.restore_accounts().await?;
    let (state, _events) = AppState::live(JailgunConfig::default(), runtime.join("receipts"), 1024);
    let control = DaemonControl::new(runtime.clone());
    let state = state
        .with_daemon_control(control.clone())
        .with_ingest_token(Some(installation.operator_token()?))
        .with_account_supervisor(supervisor.clone());
    let addr = listener.local_addr()?;
    installation.record_address(addr)?;
    eprintln!("Dashboard: http://{addr}\nRun jailgun setup to pair an operator browser.");
    let engine = supervisor.clone();
    let (failed_tx, failed_rx) = tokio::sync::oneshot::channel();
    let scheduler = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));
        loop {
            interval.tick().await;
            if engine.tick().await.is_err() {
                eprintln!("workflow-scheduler-failed: stopping scheduling; inspect the private runtime before restarting");
                let _ = failed_tx.send(());
                break;
            }
        }
    });
    use std::future::IntoFuture;
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let server = axum::serve(listener, router_with_static(state, dashboard))
        .with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
        })
        .into_future();
    tokio::pin!(server);
    let mut failed = false;
    let result = tokio::select! {
        result=&mut server=>Some(result),
        _=shutdown_signal()=>None,
        _=control.stopped()=>None,
        _=failed_rx=>{failed=true;None},
    };
    let _ = shutdown_tx.send(());
    scheduler.abort();
    let _ = scheduler.await;
    let result = match result {
        Some(result) => result,
        None => tokio::time::timeout(std::time::Duration::from_secs(5), &mut server)
            .await
            .unwrap_or(Ok(())),
    };
    supervisor.shutdown().await;
    store.close().await?;
    result?;
    anyhow::ensure!(!failed,"workflow-scheduler-failed: scheduling stopped; inspect the private runtime before restarting");
    Ok(())
}

fn now() -> i64 {
    (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! { _=tokio::signal::ctrl_c()=>{}, _=terminate.recv()=>{} }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

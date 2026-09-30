use anyhow::{Context, Result};
use jailgun_core::{installation::Installation, JailgunConfig};
use jailgun_server::{api_router, AppState, DaemonControl, WorkerService};
use std::{net::SocketAddr, path::PathBuf};

pub async fn serve(runtime: Option<PathBuf>, addr: SocketAddr) -> Result<()> {
    anyhow::ensure!(
        addr.ip().is_loopback(),
        "loopback-required: forward the worker over SSH"
    );
    let runtime = runtime
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|path| path.join(".jailgun-worker"))
        })
        .context("HOME is required or set --runtime")?;
    let installation = Installation::acquire(&runtime)?;
    let token = installation.operator_token()?;
    let worker = WorkerService::process(&runtime)?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("worker-address-busy: connect to the existing worker or choose another port")?;
    let address = listener.local_addr()?;
    installation.record_address(address)?;
    let (state, _events) = AppState::live(JailgunConfig::default(), runtime.join("receipts"), 64);
    let control = DaemonControl::new(runtime.clone());
    let state = state
        .with_ingest_token(Some(token))
        .with_daemon_control(control.clone())
        .with_worker(worker);
    eprintln!(
        "Jailgun worker: http://{address}/mcp\nCredential: {}\nRemote access: SSH local forward only.",
        runtime.join("operator-token").display()
    );
    use std::future::IntoFuture;
    let server = axum::serve(listener, api_router(state))
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = shutdown_signal() => {},
                _ = control.stopped() => {},
            }
        })
        .into_future();
    server.await?;
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

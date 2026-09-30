use super::{
    client::Client, connect::runtime_root, error::failure, operations::print_value,
    options::ConnectionOptions,
};
use anyhow::Result;
use jailgun_core::installation::{Installation, ServiceState, ServiceStatus, ServiceStopRequest};
use std::{path::Path, time::Duration};

#[derive(Debug, clap::Subcommand)]
pub enum ServiceCommand {
    /// Write a native user-service definition for review and explicit activation.
    Unit(super::startup::UnitOptions),
    /// Inspect the authenticated daemon without starting it.
    Status,
    /// Gracefully stop the observed daemon, retaining accounts and results.
    Stop,
}

pub async fn run(
    mut options: ConnectionOptions,
    command: ServiceCommand,
    json: bool,
) -> Result<bool> {
    if let ServiceCommand::Unit(unit) = command {
        return super::startup::write(options, unit, json);
    }
    options.no_start = true;
    let local_root = if options.url.is_none() {
        Some(runtime_root(&options)?)
    } else {
        None
    };
    if let Some(root) = &local_root {
        if !owned(root)? {
            print(
                &ServiceStatus {
                    state: ServiceState::Stopped,
                    instance_id: None,
                    runtime: root.clone(),
                    pid: None,
                    version: env!("CARGO_PKG_VERSION").into(),
                },
                json,
            )?;
            return Ok(true);
        }
    }
    let client = Client::connect(&options).await?;
    let mut status: ServiceStatus = client.get("/api/service").await?;
    if let Some(root) = &local_root {
        if root.canonicalize()? != status.runtime {
            return Err(failure(
                "service-runtime-mismatch",
                "The daemon owns a different runtime.",
                "Inspect the runtime descriptor and use the intended runtime.",
            ));
        }
    }
    if matches!(command, ServiceCommand::Stop) {
        let instance_id = status.instance_id.clone().ok_or_else(|| {
            failure(
                "service-instance-missing",
                "The daemon did not identify its instance.",
                "Use matching client and daemon versions.",
            )
        })?;
        let stopped: ServiceStatus = client
            .post(
                "/api/service/stop",
                &ServiceStopRequest {
                    instance_id: instance_id.clone(),
                },
            )
            .await?;
        if stopped.instance_id.as_deref() != Some(&instance_id)
            || stopped.state != ServiceState::Stopping
            || stopped.runtime != status.runtime
        {
            return Err(failure(
                "service-response-invalid",
                "Shutdown was not acknowledged by the expected instance.",
                "Inspect service status before retrying.",
            ));
        }
        status = stopped;
        if let Some(root) = &local_root {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
            while owned(root)? {
                if tokio::time::Instant::now() >= deadline {
                    return Err(failure("service-stop-incomplete", "The runtime is still owned after requesting shutdown.", "Inspect service status and its process manager before backing up or upgrading."));
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            status.state = ServiceState::Stopped;
            status.pid = None;
        }
    }
    print(&status, json)?;
    Ok(true)
}

fn owned(root: &Path) -> Result<bool> {
    Installation::is_owned(root).map_err(|_| {
        failure(
            "runtime-ownership-unavailable",
            "The existing runtime ownership lock cannot be inspected.",
            "Inspect runtime permissions and lock paths; no process was signalled.",
        )
    })
}

fn print(status: &ServiceStatus, json: bool) -> Result<()> {
    print_value(&serde_json::to_value(status)?, json)
}

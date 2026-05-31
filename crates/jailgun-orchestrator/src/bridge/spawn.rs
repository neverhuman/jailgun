//! Spawn the Node chrome-bridge child process and wire NDJSON IO.

use std::{collections::BTreeMap, process::Stdio};

use tokio::{
    process::{Child, Command},
    sync::mpsc,
};

use crate::errors::OrchestratorError;

use super::{
    protocol::{Envelope, ProtocolError},
    reader::start_stdout_reader,
    writer::start_stdin_writer,
};

pub struct BridgeSpawnConfig {
    pub command: Vec<String>,
    pub env: BTreeMap<String, String>,
}

pub struct BridgeHandle {
    pub child: Child,
    /// Send envelopes here to forward them to the bridge's stdin.
    pub commands_tx: mpsc::Sender<Envelope<serde_json::Value>>,
    /// Receive every envelope the bridge emits on its stdout here.
    pub events_rx: mpsc::UnboundedReceiver<Result<Envelope<serde_json::Value>, ProtocolError>>,
}

impl BridgeHandle {
    pub async fn shutdown(mut self) -> Result<(), OrchestratorError> {
        drop(self.commands_tx);
        match self.child.wait().await {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(OrchestratorError::BridgeExited(status.code())),
            Err(error) => Err(OrchestratorError::Io(error)),
        }
    }
}

pub async fn spawn_bridge(cfg: BridgeSpawnConfig) -> Result<BridgeHandle, OrchestratorError> {
    if cfg.command.is_empty() {
        return Err(OrchestratorError::Config(
            "bridge_cmd cannot be empty".into(),
        ));
    }
    let mut command = Command::new(&cfg.command[0]);
    if cfg.command.len() > 1 {
        command.args(&cfg.command[1..]);
    }
    command.envs(cfg.env.iter().map(|(k, v)| (k.clone(), v.clone())));
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command
        .spawn()
        .map_err(|error| OrchestratorError::BridgeSpawn(error.to_string()))?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| OrchestratorError::BridgeSpawn("child stdin unavailable".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| OrchestratorError::BridgeSpawn("child stdout unavailable".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| OrchestratorError::BridgeSpawn("child stderr unavailable".into()))?;

    let (commands_tx, commands_rx) = mpsc::channel(64);
    let events_rx = start_stdout_reader(stdout);
    start_stdin_writer(stdin, commands_rx);
    drain_stderr(stderr);

    Ok(BridgeHandle {
        child,
        commands_tx,
        events_rx,
    })
}

fn drain_stderr(stderr: tokio::process::ChildStderr) {
    tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut reader = BufReader::new(stderr).lines();
        loop {
            match reader.next_line().await {
                Ok(Some(line)) => tracing::debug!(target: "chrome-bridge", "{}", line),
                Ok(None) => break,
                Err(error) => {
                    tracing::warn!(?error, "bridge stderr read error");
                    break;
                }
            }
        }
    });
}

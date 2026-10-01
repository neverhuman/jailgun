//! Spawn the Node chrome-bridge child process and wire NDJSON IO.

use std::{collections::BTreeMap, ffi::OsString, process::Stdio};

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
    pub events_rx: mpsc::Receiver<Result<Envelope<serde_json::Value>, ProtocolError>>,
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
    command
        .env_clear()
        .envs(browser_environment(std::env::vars_os()));
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
        use tokio::io::AsyncReadExt;
        let mut reader = stderr;
        let mut buffer = [0; 8192];
        let mut reported = false;
        loop {
            match reader.read(&mut buffer).await {
                Ok(0) => break,
                Ok(_) if !reported => {
                    // Raw browser diagnostics can include page content or login data.
                    // Named operation failures travel over the structured protocol.
                    tracing::debug!(target: "chrome-bridge", "browser emitted private diagnostics; contents discarded");
                    reported = true;
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(?error, "bridge stderr read error");
                    break;
                }
            }
        }
    });
}

// Deliberate inherited environment: Node hooks, shell startup files, SSH agents,
// provider credentials and unrelated application secrets never enter the bridge.
fn browser_environment(
    values: impl Iterator<Item = (OsString, OsString)>,
) -> Vec<(OsString, OsString)> {
    const ALLOWED: &[&str] = &[
        "PATH",
        "HOME",
        "USER",
        "LOGNAME",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "TMPDIR",
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XDG_RUNTIME_DIR",
        "XDG_DOWNLOAD_DIR",
        "XAUTHORITY",
        "GOOGLE_CHROME_EXECUTABLE",
        "JAILGUN_CHROME_EXECUTABLE",
    ];
    values
        .filter(|(key, _)| key.to_str().is_some_and(|key| ALLOWED.contains(&key)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_inherits_only_browser_prerequisites() {
        let inherited = browser_environment(
            [
                ("PATH", "/usr/bin"),
                ("DISPLAY", ":99"),
                ("HOME", "/synthetic"),
                ("GH_TOKEN", "secret"),
                ("TELEGRAM_BOT_TOKEN", "secret"),
                ("JAILGUN_INGEST_TOKEN", "secret"),
                ("NODE_OPTIONS", "--require injected.js"),
                ("SSH_AUTH_SOCK", "/agent.sock"),
                ("UNRELATED_SECRET", "secret"),
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), v.into())),
        );
        assert_eq!(inherited.len(), 3);
        assert!(inherited
            .iter()
            .all(|(k, _)| ["PATH", "DISPLAY", "HOME"].contains(&k.to_str().unwrap())));
    }
}

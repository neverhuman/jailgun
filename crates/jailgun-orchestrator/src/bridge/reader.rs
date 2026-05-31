//! NDJSON stdout reader for the chrome-bridge child.

use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::ChildStdout,
    sync::mpsc,
};

use super::protocol::{decode_envelope, Envelope, ProtocolError};

const BRIDGE_STDOUT_QUEUE: usize = 128;

pub fn start_stdout_reader(
    stdout: ChildStdout,
) -> mpsc::Receiver<Result<Envelope<serde_json::Value>, ProtocolError>> {
    let (tx, rx) = mpsc::channel(BRIDGE_STDOUT_QUEUE);
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let result = decode_envelope(&line);
                    if tx.send(result).await.is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    tracing::warn!(?error, "bridge stdout read error");
                    let _ = tx
                        .send(Err(ProtocolError::Decode(serde_json::Error::io(error))))
                        .await;
                    break;
                }
            }
        }
    });
    rx
}

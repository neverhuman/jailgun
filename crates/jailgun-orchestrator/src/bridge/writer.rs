//! NDJSON stdin writer for the chrome-bridge child.

use tokio::{io::AsyncWriteExt, process::ChildStdin, sync::mpsc};

use super::protocol::{encode_envelope, Envelope};

pub fn start_stdin_writer(
    mut stdin: ChildStdin,
    mut rx: mpsc::Receiver<Envelope<serde_json::Value>>,
) {
    tokio::spawn(async move {
        while let Some(envelope) = rx.recv().await {
            match encode_envelope(&envelope) {
                Ok(line) => {
                    if let Err(error) = stdin.write_all(line.as_bytes()).await {
                        tracing::warn!(?error, "bridge stdin write failed");
                        break;
                    }
                    if let Err(error) = stdin.flush().await {
                        tracing::warn!(?error, "bridge stdin flush failed");
                        break;
                    }
                }
                Err(error) => {
                    tracing::warn!(?error, "envelope encode failed; dropping");
                }
            }
        }
        let _ = stdin.shutdown().await;
    });
}

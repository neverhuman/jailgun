use crate::bridge::{spawn_bridge, BridgeSpawnConfig, Envelope};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::{mpsc, oneshot, Mutex};

#[derive(Debug, Clone, Deserialize, thiserror::Error)]
#[error("{code}: {message}; {next_action}")]
pub struct BrowserFailure {
    pub code: String,
    pub message: String,
    pub next_action: String,
    #[serde(default)]
    pub retry_at_ms: Option<i64>,
    #[serde(default)]
    pub rate_limited: bool,
}

impl BrowserFailure {
    fn unavailable() -> Self {
        Self {
            code: "browser-disconnected".into(),
            message: "The account browser supervisor disconnected.".into(),
            next_action: "Reconcile accepted conversations before submitting again.".into(),
            retry_at_ms: None,
            rate_limited: false,
        }
    }
}

type Reply = oneshot::Sender<Result<Value, BrowserFailure>>;
struct Pending {
    run_id: String,
    reply: Reply,
}

/// A single supervised bridge process is shared by all runs for one account.
#[derive(Clone)]
pub struct ConceptBridge {
    child: Arc<Mutex<tokio::process::Child>>,
    commands: mpsc::Sender<Envelope<Value>>,
    pending: Arc<Mutex<HashMap<String, Pending>>>,
}

impl ConceptBridge {
    pub async fn is_alive(&self) -> bool {
        matches!(self.child.lock().await.try_wait(), Ok(None))
    }
    pub async fn spawn(config: BridgeSpawnConfig, initialize: Value) -> anyhow::Result<Self> {
        let handle = spawn_bridge(config).await?;
        let mut events = handle.events_rx;
        let pending: Arc<Mutex<HashMap<String, Pending>>> = Arc::new(Mutex::new(HashMap::new()));
        let responses = pending.clone();
        tokio::spawn(async move {
            while let Some(envelope) = events.recv().await {
                let Ok(envelope) = envelope else { break };
                let Some(id) = envelope.correlation_id else {
                    continue;
                };
                let Some(pending) = responses.lock().await.remove(&id) else {
                    continue;
                };
                let result = if envelope.run_id != pending.run_id {
                    Err(BrowserFailure::unavailable())
                } else if envelope.kind == "concept.result" {
                    Ok(envelope.payload)
                } else {
                    Err(serde_json::from_value(envelope.payload)
                        .unwrap_or_else(|_| BrowserFailure::unavailable()))
                };
                let _ = pending.reply.send(result);
            }
            for (_, pending) in responses.lock().await.drain() {
                let _ = pending.reply.send(Err(BrowserFailure::unavailable()));
            }
        });
        let bridge = Self {
            child: Arc::new(Mutex::new(handle.child)),
            commands: handle.commands_tx,
            pending,
        };
        bridge
            .call(
                "concept.initialize",
                "account",
                initialize,
                Duration::from_secs(60),
            )
            .await?;
        Ok(bridge)
    }

    pub async fn call(
        &self,
        operation: &str,
        run_id: &str,
        payload: Value,
        timeout: Duration,
    ) -> Result<Value, BrowserFailure> {
        let id = uuid::Uuid::new_v4().to_string();
        let (reply, result) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            if pending.len() >= 64 {
                return Err(BrowserFailure {
                    code: "browser-queue-full".into(),
                    message: "The account browser command queue is full.".into(),
                    next_action: "Wait for in-progress browser operations.".into(),
                    retry_at_ms: None,
                    rate_limited: false,
                });
            }
            pending.insert(
                id.clone(),
                Pending {
                    run_id: run_id.into(),
                    reply,
                },
            );
        }
        let envelope = Envelope::new(
            operation,
            run_id,
            time::OffsetDateTime::now_utc().unix_timestamp().to_string(),
            payload,
        )
        .with_id(&id);
        if self.commands.send(envelope).await.is_err() {
            self.pending.lock().await.remove(&id);
            return Err(BrowserFailure::unavailable());
        }
        match tokio::time::timeout(timeout, result).await {
            Ok(Ok(result)) => result,
            _ => {
                self.pending.lock().await.remove(&id);
                Err(BrowserFailure::unavailable())
            }
        }
    }

    /// Only daemon/account shutdown calls this; an individual run never restarts the shared browser.
    pub async fn shutdown(&self) -> anyhow::Result<()> {
        self.call(
            "concept.shutdown",
            "account",
            serde_json::json!({}),
            Duration::from_secs(15),
        )
        .await?;
        let mut child = self.child.lock().await;
        child.kill().await?;
        child.wait().await?;
        Ok(())
    }
}

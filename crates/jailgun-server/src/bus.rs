//! Pluggable event bus. The orchestrator holds an `Arc<dyn EventBus>` to
//! publish `JailgunEvent`s. Production uses `BroadcastBus` so the server's WS
//! endpoint can stream them out. Tests use `NoopBus` or `RecordingBus`.

use std::sync::Mutex;

use jailgun_core::JailgunEvent;
use tokio::sync::broadcast;

pub trait EventBus: Send + Sync {
    fn publish(&self, event: JailgunEvent);
}

#[derive(Clone)]
pub struct BroadcastBus(pub broadcast::Sender<JailgunEvent>);

impl EventBus for BroadcastBus {
    fn publish(&self, event: JailgunEvent) {
        let _ = self.0.send(event);
    }
}

#[derive(Default)]
pub struct NoopBus;

impl EventBus for NoopBus {
    fn publish(&self, _event: JailgunEvent) {}
}

#[derive(Default)]
pub struct RecordingBus {
    pub events: Mutex<Vec<JailgunEvent>>,
}

impl EventBus for RecordingBus {
    fn publish(&self, event: JailgunEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }
}

impl RecordingBus {
    pub fn snapshot(&self) -> Vec<JailgunEvent> {
        self.events
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }
}

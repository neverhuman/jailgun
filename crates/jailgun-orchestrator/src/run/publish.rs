use jailgun_core::{EventKind, JailgunEvent, Severity};
use tokio::sync::broadcast;

pub(super) fn publish(events: &broadcast::Sender<JailgunEvent>, event: JailgunEvent) {
    let _ = events.send(event);
}

pub(super) fn publish_browser_log<I, K, V>(
    events: &broadcast::Sender<JailgunEvent>,
    run_id: &str,
    tab_id: Option<u16>,
    phase: &str,
    status: &str,
    message: &str,
    fields: I,
) where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
{
    let mut event = JailgunEvent::new(run_id.to_string(), EventKind::BrowserLog, message)
        .with_field("phase", phase.to_string())
        .with_field("status", status.to_string());
    if let Some(tab_id) = tab_id {
        event = event.with_tab(tab_id);
    }
    for (key, value) in fields {
        event = event.with_field(key, value);
    }
    publish(events, event);
}

pub(super) fn publish_error(
    events: &broadcast::Sender<JailgunEvent>,
    run_id: &str,
    tab_id: Option<u16>,
    error: impl Into<String>,
) {
    let mut event = JailgunEvent::new(run_id.to_string(), EventKind::Error, error.into())
        .with_severity(Severity::Error);
    if let Some(tab_id) = tab_id {
        event = event.with_tab(tab_id);
    }
    publish(events, event);
}

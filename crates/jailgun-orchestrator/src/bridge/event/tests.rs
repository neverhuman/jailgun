use super::*;
use crate::bridge::protocol::{decode_envelope, encode_envelope, envelope_for_event};

#[test]
fn roundtrip_download_complete_event() {
    let payload = DownloadCompletePayload {
        sha256: "a".repeat(64),
        size_bytes: 12345,
        local_path: "/tmp/x.tar.gz".into(),
        receipt_path: "/tmp/r/x.tar.gz".into(),
        original_name: "patch.tar.gz".into(),
        local_name: "patch.tar.gz".into(),
        file_kind: Some("downloaded-archive".into()),
        download_url: Some("blob:https://chatgpt.com/x".into()),
        entry_count: None,
        download_latency_ms: None,
        started_at: "2026-05-31T12:00:00Z".into(),
        finished_at: "2026-05-31T12:00:08Z".into(),
    };
    let event = BridgeEvent::DownloadComplete(payload.clone());
    let envelope = envelope_for_event(&event, "run-test", "2026-05-31T12:00:00Z", Some(2));
    let line = encode_envelope(&envelope).expect("encode");
    let decoded = decode_envelope(line.trim_end()).expect("decode");
    let typed = BridgeEvent::decode(&decoded.kind, decoded.payload).expect("typed");
    match typed {
        BridgeEvent::DownloadComplete(got) => assert_eq!(got, payload),
        other => panic!("unexpected variant: {other:?}"),
    }
}

#[test]
fn roundtrip_auth_complete_event() {
    let payload = AuthCompletePayload {
        page_url: "https://chatgpt.com/".into(),
        composer_detected: true,
    };
    let event = BridgeEvent::AuthComplete(payload.clone());
    let envelope = envelope_for_event(&event, "auth-run", "2026-05-31T12:00:00Z", None);
    let line = encode_envelope(&envelope).expect("encode");
    let decoded = decode_envelope(line.trim_end()).expect("decode");
    assert_eq!(decoded.kind, "auth-complete");
    let typed = BridgeEvent::decode(&decoded.kind, decoded.payload).expect("typed");
    match typed {
        BridgeEvent::AuthComplete(got) => assert_eq!(got, payload),
        other => panic!("unexpected variant: {other:?}"),
    }
}

#[test]
fn unknown_event_kind_returns_protocol_error() {
    let err = BridgeEvent::decode("ghost-event", serde_json::json!({})).expect_err("unknown");
    assert!(matches!(err, ProtocolError::UnknownEvent(_)));
}

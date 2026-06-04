use super::*;

#[tokio::test]
async fn serves_run_snapshot_and_redacted_config() {
    let app = api_router(AppState::fixture(JailgunConfig::default()));
    let runs_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/runs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(runs_response.status(), StatusCode::OK);

    let config_response = app
        .oneshot(
            Request::builder()
                .uri("/api/config/effective")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(config_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn ingest_requires_matching_token() {
    let (state, _rx) = AppState::live(JailgunConfig::default(), PathBuf::from("receipts"), 64);
    let history = state.events.clone();
    let app = api_router(state.with_ingest_token(Some("secret".to_string())));

    let event = JailgunEvent::new("run-1", EventKind::RunStarted, "hi");
    let body = serde_json::to_vec(&event).unwrap();

    let bad = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/events")
                .header("content-type", "application/json")
                .header("x-jailgun-token", "wrong")
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);

    let ok = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/events")
                .header("content-type", "application/json")
                .header("x-jailgun-token", "secret")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::ACCEPTED);
    let events = history.read().await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].run_id, "run-1");
}

#[tokio::test]
async fn ingest_without_token_returns_503() {
    let (state, _rx) = AppState::live(JailgunConfig::default(), PathBuf::from("receipts"), 64);
    let app = api_router(state);
    let event = JailgunEvent::new("run-1", EventKind::RunStarted, "hi");
    let body = serde_json::to_vec(&event).unwrap();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/events")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn websocket_events_require_configured_token() {
    let (state, _rx) = AppState::live(JailgunConfig::default(), PathBuf::from("receipts"), 64);
    let state = Arc::new(state.with_ingest_token(Some("secret".into())));

    let unauthorized = websocket_unauthorized(
        &state,
        &HeaderMap::new(),
        &WsAuthQuery {
            token: Some("wrong".into()),
            jailgun_token: None,
        },
    )
    .expect("bad token rejected");
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let query_ok = websocket_unauthorized(
        &state,
        &HeaderMap::new(),
        &WsAuthQuery {
            token: Some("secret".into()),
            jailgun_token: None,
        },
    );
    assert!(query_ok.is_none());

    let mut headers = HeaderMap::new();
    headers.insert("x-jailgun-token", "secret".parse().unwrap());
    assert!(websocket_unauthorized(&state, &headers, &WsAuthQuery::default()).is_none());

    let (state, _rx) = AppState::live(JailgunConfig::default(), PathBuf::from("receipts"), 64);
    let unavailable =
        websocket_unauthorized(&Arc::new(state), &HeaderMap::new(), &WsAuthQuery::default())
            .expect("missing server token rejected");
    assert_eq!(unavailable.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn live_bus_forwards_events_to_receivers() {
    let (state, mut rx) = AppState::live(JailgunConfig::default(), PathBuf::from("receipts"), 64);
    let tx = state.event_bus.clone().expect("live bus");
    let event = JailgunEvent::new("run-A", EventKind::DeployFinished, "ok");
    tx.send(event.clone()).expect("send ok");
    let received = rx.recv().await.expect("recv ok");
    assert_eq!(received, event);
}

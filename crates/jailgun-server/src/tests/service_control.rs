use super::*;
use crate::DaemonControl;

async fn request(
    app: &Router,
    path: &str,
    body: Value,
    credential: (&str, &str),
    origin: Option<&str>,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header("host", "localhost")
        .header("content-type", "application/json")
        .header(credential.0, credential.1);
    if let Some(origin) = origin {
        request = request.header("origin", origin);
    }
    let response = app
        .clone()
        .oneshot(
            request
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn shutdown_requires_the_observed_instance_and_rejects_further_mutations() {
    let control = DaemonControl::new(PathBuf::from("/synthetic/runtime"));
    let state = AppState::fixture(JailgunConfig::default())
        .with_ingest_token(Some("synthetic-operator".into()))
        .with_daemon_control(control.clone());
    let code = state.dashboard_sessions.issue_pairing_code();
    let app = api_router(state);
    let auth = ("authorization", "Bearer synthetic-operator");
    for (body, expected) in [
        (json!({"instance_id":"stale"}), StatusCode::CONFLICT),
        (json!({}), StatusCode::BAD_REQUEST),
    ] {
        assert_eq!(
            request(&app, "/api/service/stop", body, auth, None).await.0,
            expected
        );
        assert!(!control.is_stopping());
    }
    let body = json!({"instance_id": control.status().instance_id});
    assert_eq!(
        request(
            &app,
            "/api/service/stop",
            body.clone(),
            auth,
            Some("http://evil.invalid")
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert!(!control.is_stopping());
    let paired = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/pair")
                .header("host", "localhost")
                .header("origin", "http://localhost")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({"code":code})).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(paired.status(), StatusCode::NO_CONTENT);
    let cookie = paired.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    assert_eq!(
        request(
            &app,
            "/api/service/stop",
            body.clone(),
            ("cookie", cookie),
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert!(!control.is_stopping());
    let stopped = request(
        &app,
        "/api/service/stop",
        body.clone(),
        ("cookie", cookie),
        Some("http://localhost"),
    )
    .await;
    assert_eq!(stopped.0, StatusCode::ACCEPTED);
    assert_eq!(stopped.1["state"], "stopping");
    assert_eq!(stopped.1["instance_id"], body["instance_id"]);
    tokio::time::timeout(std::time::Duration::from_secs(1), control.stopped())
        .await
        .unwrap();
    assert_eq!(
        request(&app, "/api/service/stop", body, auth, None).await.0,
        StatusCode::ACCEPTED
    );
    let rejected = request(&app, "/api/concept-runs", json!({}), auth, None).await;
    assert_eq!(rejected.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(rejected.1["code"], "service-stopping");
    let next = DaemonControl::new(PathBuf::from("/synthetic/runtime"));
    assert_ne!(next.status().instance_id, control.status().instance_id);
    assert!(!next.is_stopping());
}

use super::*;

fn private_app() -> (AppState, Router) {
    let state = AppState::fixture(JailgunConfig::default())
        .with_ingest_token(Some("operator-token-for-test".into()));
    let app = api_router(state.clone());
    (state, app)
}

#[tokio::test]
async fn every_private_route_rejects_unauthenticated_requests() {
    let (_, app) = private_app();
    for (method, path) in [
        ("GET", "/api/service"),
        ("POST", "/api/service/stop"),
        ("POST", "/api/session/pairing-code"),
        ("GET", "/api/runs"),
        ("GET", "/api/tokens"),
        ("POST", "/api/tokens"),
        ("DELETE", "/api/tokens/token-id"),
        ("POST", "/api/runs"),
        ("POST", "/api/concept-runs"),
        ("GET", "/api/accounts"),
        ("GET", "/api/accounts/sessions"),
        ("POST", "/api/accounts/connect"),
        ("POST", "/api/accounts/account/reconnect"),
        ("POST", "/api/accounts/account/confirm"),
        ("POST", "/api/accounts/account/cancel-login"),
        ("GET", "/api/accounts/account/login-view"),
        ("GET", "/api/runs/run-1/result"),
        ("GET", "/api/runs/run-1/attempts"),
        ("GET", "/api/runs/run-1/events"),
        ("GET", "/api/runs/run-1/artifacts/artifact-1"),
        ("POST", "/api/runs/run-1/pause"),
        ("POST", "/api/runs/run-1/resume"),
        ("POST", "/api/runs/run-1/cancel"),
        ("GET", "/api/runs/run-1"),
        ("GET", "/api/runs/run-1/agent-summary"),
        ("GET", "/api/receipts/run-1"),
        ("GET", "/api/config/effective"),
        ("GET", "/api/browser/accounts"),
        ("GET", "/api/browsers"),
        ("GET", "/api/browsers/account/auth/status"),
        ("POST", "/api/browsers/account/start"),
        ("POST", "/api/browsers/account/auth/start"),
        ("POST", "/api/browsers/account/auth/code"),
        ("POST", "/api/browsers/account/restart"),
        ("POST", "/api/browsers/account/stop"),
        ("POST", "/api/events"),
        ("POST", "/mcp"),
        ("GET", "/ws/events"),
    ] {
        for token in [None, Some("incorrect")] {
            let mut request = Request::builder()
                .method(method)
                .uri(path)
                .header("host", "localhost");
            if let Some(token) = token {
                request = request.header("authorization", format!("Bearer {token}"));
            }
            let response = app
                .clone()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::UNAUTHORIZED,
                "{method} {path}"
            );
        }
    }
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({"status":"ok"})
    );
}

#[tokio::test]
async fn login_view_requires_operator_cookie_origin_and_no_query() {
    let (state, app) = private_app();
    let code = state.dashboard_sessions.issue_pairing_code();
    let response = app
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
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    for (credential, origin, query, expected, code) in [
        (
            false,
            Some("http://localhost"),
            "",
            StatusCode::FORBIDDEN,
            "operator-session-required",
        ),
        (true, None, "", StatusCode::FORBIDDEN, "origin-required"),
        (
            true,
            Some("http://localhost:9999"),
            "",
            StatusCode::FORBIDDEN,
            "origin-rejected",
        ),
        (
            true,
            Some("http://localhost"),
            "?token=removed",
            StatusCode::BAD_REQUEST,
            "login-view-query-rejected",
        ),
        (
            true,
            Some("http://localhost"),
            "",
            StatusCode::SERVICE_UNAVAILABLE,
            "workflow-unavailable",
        ),
    ] {
        let mut request = Request::builder()
            .uri(format!("/api/accounts/test/login-view{query}"))
            .header("host", "localhost");
        request = if credential {
            request.header("cookie", &cookie)
        } else {
            request.header("authorization", "Bearer operator-token-for-test")
        };
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        let response = app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        let bytes = axum::body::to_bytes(response.into_body(), 2048)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap()["code"],
            code
        );
    }
}

#[tokio::test]
async fn pairing_is_single_use_and_cookie_authorizes_reads() {
    let (_, app) = private_app();
    let issued = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/pairing-code")
                .header("host", "localhost")
                .header("authorization", "Bearer operator-token-for-test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(issued.status(), StatusCode::OK);
    assert_eq!(issued.headers()["cache-control"], "no-store");
    let bytes = axum::body::to_bytes(issued.into_body(), 1024)
        .await
        .unwrap();
    let issued: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(issued["expires_in_seconds"], 300);
    let code = issued["code"].as_str().unwrap();
    let body = serde_json::to_vec(&json!({"code":code})).unwrap();
    let request = || {
        Request::builder()
            .method("POST")
            .uri("/api/session/pair")
            .header("host", "localhost:8080")
            .header("origin", "http://localhost:8080")
            .header("content-type", "application/json")
            .body(Body::from(body.clone()))
            .unwrap()
    };
    let paired = app.clone().oneshot(request()).await.unwrap();
    assert_eq!(paired.status(), StatusCode::NO_CONTENT);
    let cookie = paired.headers()["set-cookie"].to_str().unwrap();
    assert!(cookie.contains("HttpOnly; SameSite=Strict; Path=/; Max-Age="));
    let cookie = cookie.split(';').next().unwrap().to_string();
    let replay = app.clone().oneshot(request()).await.unwrap();
    assert_eq!(replay.status(), StatusCode::UNAUTHORIZED);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/runs")
                .header("host", "localhost:8080")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let mutation = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/events")
                .header("host", "localhost:8080")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(mutation.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn rejects_host_origin_and_conflicting_credentials() {
    let (_, app) = private_app();
    for (host, origin) in [
        ("evil.invalid", "http://evil.invalid"),
        ("localhost:8080", "http://evil.invalid"),
        ("localhost:8080", "null"),
        ("localhost:8080", "http://localhost:8081"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/runs")
                    .header("host", host)
                    .header("origin", origin)
                    .header("authorization", "Bearer operator-token-for-test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/runs")
                .header("host", "localhost")
                .header("authorization", "Bearer operator-token-for-test")
                .header("x-jailgun-token", "different")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

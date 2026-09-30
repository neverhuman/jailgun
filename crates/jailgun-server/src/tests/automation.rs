use super::*;
use jailgun_workflow::model::*;

async fn call(
    app: &Router,
    secret: &str,
    method: &str,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("host", "localhost")
                .header("authorization", format!("Bearer {secret}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn automation_is_account_scoped_and_cannot_reach_operator_actions() {
    let (_directory, store, app) = super::concept_routes::fixture().await;
    let account = store
        .connect_account(ConnectAccount {
            email: "second@example.invalid".into(),
            id: Some("second".into()),
        })
        .await
        .unwrap();
    let other = store
        .submit(
            ConceptRequest {
                concept: "Another private concept".into(),
                account_id: account.id,
                candidate_count: 5,
                constraints: String::new(),
                criteria: default_criteria(),
                idempotency_key: "other-key".into(),
            },
            ModelSelection::Current,
            crate::concepts::now(),
        )
        .await
        .unwrap();
    let issued = call(
        &app,
        "synthetic-token",
        "POST",
        "/api/tokens",
        json!({"name":"Automation","account_ids":["synthetic"]}),
    )
    .await;
    assert_eq!(issued.0, StatusCode::OK);
    let secret = issued.1["secret"].as_str().unwrap();
    let token_id = issued.1["metadata"]["id"].as_str().unwrap();
    let accounts = call(&app, secret, "GET", "/api/accounts", Value::Null).await;
    assert_eq!(accounts.0, StatusCode::OK);
    assert_eq!(accounts.1.as_array().unwrap().len(), 1);
    assert_eq!(accounts.1[0]["id"], "synthetic");
    let rejected = call(
        &app,
        secret,
        "POST",
        "/api/concept-runs",
        json!({"concept":"Forbidden account","account_id":"second","idempotency_key":"forbidden"}),
    )
    .await;
    assert_eq!(rejected.0, StatusCode::FORBIDDEN);
    // A matching existing key never bypasses account authorization.
    assert_eq!(
        call(
            &app,
            secret,
            "POST",
            "/api/concept-runs",
            serde_json::to_value(&other.request).unwrap()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let accepted=call(&app,secret,"POST","/api/concept-runs",json!({"concept":"A permitted synthetic concept","account_id":"synthetic","idempotency_key":"permitted"})).await;
    assert_eq!(accepted.0, StatusCode::ACCEPTED);
    let id = accepted.1["run_id"].as_str().unwrap();
    let listed = call(&app, secret, "GET", "/api/runs", Value::Null).await;
    assert_eq!(listed.1.as_array().unwrap().len(), 1);
    assert_eq!(listed.1[0]["id"], id);
    for suffix in ["", "/result", "/attempts", "/events", "/artifacts/any"] {
        assert_eq!(
            call(
                &app,
                secret,
                "GET",
                &format!("/api/runs/{}{suffix}", other.id),
                Value::Null
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
    for action in ["pause", "resume", "cancel"] {
        assert_eq!(
            call(
                &app,
                secret,
                "POST",
                &format!("/api/runs/{}/{action}", other.id),
                json!({})
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            call(
                &app,
                secret,
                "POST",
                &format!("/api/runs/{id}/{action}"),
                json!({})
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    for (method, path) in [
        ("POST", "/api/session/pairing-code"),
        ("GET", "/api/service"),
        ("POST", "/api/service/stop"),
        ("GET", "/api/tokens"),
        ("POST", "/api/tokens"),
        ("GET", "/api/accounts/sessions"),
        ("GET", "/api/accounts/synthetic/login-view"),
        ("POST", "/api/accounts/connect"),
        ("POST", "/api/accounts/synthetic/reconnect"),
        ("POST", "/api/accounts/synthetic/confirm"),
        ("POST", "/api/accounts/synthetic/cancel-login"),
        ("GET", "/api/config/effective"),
        ("GET", "/api/runs?kind=archive"),
        ("POST", "/api/runs"),
        ("GET", "/api/browser/accounts"),
        ("POST", "/api/browsers/synthetic/auth/start"),
        ("POST", "/api/browsers/synthetic/start"),
        ("POST", "/api/browsers/synthetic/stop"),
        ("POST", "/api/browsers/synthetic/restart"),
        ("POST", "/api/events"),
        ("GET", "/api/receipts/any"),
        ("GET", "/ws/events"),
    ] {
        assert_eq!(
            call(&app, secret, method, path, json!({})).await.0,
            StatusCode::FORBIDDEN,
            "{method} {path}"
        );
    }
    assert_eq!(
        call(
            &app,
            secret,
            "DELETE",
            &format!("/api/tokens/{token_id}"),
            Value::Null
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            &app,
            "synthetic-token",
            "DELETE",
            &format!("/api/tokens/{token_id}"),
            Value::Null
        )
        .await
        .0,
        StatusCode::OK
    );
    for (method, path) in [
        ("GET", "/api/runs"),
        ("GET", "/api/accounts"),
        ("POST", "/api/concept-runs"),
    ] {
        assert_eq!(
            call(&app, secret, method, path, json!({})).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
}

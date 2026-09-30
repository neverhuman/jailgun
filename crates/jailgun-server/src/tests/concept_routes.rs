use super::*;
use jailgun_workflow::{model::*, Store};

pub(super) async fn fixture() -> (tempfile::TempDir, Store, Router) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/concept-api-proofs");
    std::fs::create_dir_all(&root).unwrap();
    let temp = tempfile::Builder::new().tempdir_in(root).unwrap();
    let registry_path =
        write_browser_registry(temp.path(), &[("synthetic", BrowserAccountStatus::Ready)]);
    BrowserProfileRegistry::update(&registry_path, |registry| {
        registry
            .account_mut("synthetic")
            .unwrap()
            .provider_account_id = Some("synthetic-identity".into());
        Ok(())
    })
    .unwrap();
    let store = Store::open(temp.path().join("runtime")).unwrap();
    store
        .import_registry(registry_path, crate::concepts::now())
        .await
        .unwrap();
    let app = api_router(
        AppState::fixture(JailgunConfig::default())
            .with_ingest_token(Some("synthetic-token".into()))
            .with_workflow(store.clone()),
    );
    (temp, store, app)
}

async fn call(app: &Router, method: &str, path: &str, value: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("host", "localhost")
                .header("authorization", "Bearer synthetic-token")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&value).unwrap()))
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
fn request() -> Value {
    json!({"concept":"A synthetic neighborhood library","account_id":"synthetic","idempotency_key":"synthetic-request"})
}

#[tokio::test]
async fn concept_submission_idempotency_controls_events_and_restart_are_durable() {
    let (temp, store, app) = fixture().await;
    let (status, accepted) = call(&app, "POST", "/api/concept-runs", request()).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let id = accepted["run_id"].as_str().unwrap();
    assert_eq!(accepted["result_url"], format!("/api/runs/{id}/result"));
    assert_eq!(
        call(&app, "POST", "/api/concept-runs", request()).await.1,
        accepted
    );
    // Simulate a lost response: discard the response body, then retry exactly.
    let _ = call(&app, "POST", "/api/concept-runs", request()).await;
    assert_eq!(
        call(&app, "POST", "/api/concept-runs", request()).await.1,
        accepted
    );
    let mut different = request();
    different["concept"] = json!("Different content");
    let conflict = call(&app, "POST", "/api/concept-runs", different).await;
    assert_eq!(conflict.0, StatusCode::CONFLICT);
    assert_eq!(conflict.1["code"], "idempotency-conflict");
    assert_eq!(
        call(&app, "GET", "/api/runs", Value::Null)
            .await
            .1
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        call(&app, "GET", "/api/runs?kind=archive", Value::Null)
            .await
            .1[0]["run_id"],
        "fixture-run"
    );
    for (action, body, status) in [
        ("pause", json!({}), "paused"),
        ("resume", json!({}), "queued"),
        ("cancel", json!({}), "cancelled"),
        ("cancel", json!({}), "cancelled"),
    ] {
        let result = call(&app, "POST", &format!("/api/runs/{id}/{action}"), body).await;
        assert_eq!(result.0, StatusCode::OK);
        assert_eq!(result.1["status"], status);
    }
    assert_eq!(
        call(&app, "GET", &format!("/api/runs/{id}/result"), Value::Null)
            .await
            .1["code"],
        "result-not-complete"
    );
    let events = call(&app, "GET", &format!("/api/runs/{id}/events"), Value::Null)
        .await
        .1;
    let events = events.as_array().unwrap();
    assert!(events
        .windows(2)
        .all(|pair| pair[0]["sequence"].as_i64().unwrap() < pair[1]["sequence"].as_i64().unwrap()));
    let cursor = events[events.len() - 2]["sequence"].as_i64().unwrap();
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("/api/runs/{id}/events?after={cursor}"),
            Value::Null
        )
        .await
        .1
        .as_array()
        .unwrap()
        .len(),
        1
    );
    drop(app);
    store.close().await.unwrap();
    let restarted = Store::open(temp.path().join("runtime")).unwrap();
    assert_eq!(restarted.run(id.into()).await.unwrap().status, "cancelled");
    restarted.close().await.unwrap();
}

#[tokio::test]
async fn concept_input_cannot_enable_advanced_execution_or_subsets_implicitly() {
    let (_temp, store, app) = fixture().await;
    for field in ["deploy", "bridge_cmd", "prompt_file", "model"] {
        let mut invalid = request();
        invalid[field] = json!(true);
        let result = call(&app, "POST", "/api/concept-runs", invalid).await;
        assert_eq!(result.0, StatusCode::BAD_REQUEST);
        assert_eq!(result.1["code"], "invalid-json");
    }
    for count in [0, 4, 11] {
        let mut invalid = request();
        invalid["candidate_count"] = json!(count);
        assert_eq!(
            call(&app, "POST", "/api/concept-runs", invalid).await.1["code"],
            "invalid-request"
        );
    }
    let accepted = call(&app, "POST", "/api/concept-runs", request()).await.1;
    let id = accepted["run_id"].as_str().unwrap();
    call(&app, "POST", &format!("/api/runs/{id}/pause"), json!({})).await;
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/api/runs/{id}/resume"),
            json!({"allow_incomplete":true})
        )
        .await
        .1["code"],
        "subset-unavailable"
    );
    assert_eq!(
        call(&app, "POST", "/api/runs", json!({})).await.1["code"],
        "workflow-runtime-owned"
    );
    assert_eq!(
        call(&app, "POST", "/api/browsers/synthetic/restart", json!({}))
            .await
            .1["code"],
        "workflow-runtime-owned"
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn partial_artifacts_require_registration_and_verified_content() {
    let (temp, store, app) = fixture().await;
    let accepted = call(&app, "POST", "/api/concept-runs", request()).await.1;
    let id = accepted["run_id"].as_str().unwrap().to_string();
    let now = crate::concepts::now();
    let lease = store.claim(now, 0).await.unwrap().unwrap();
    store.begin_submission(lease.clone(), now).await.unwrap();
    store
        .accepted(
            lease.clone(),
            AcceptedTurn {
                conversation_id: "synthetic-conversation".into(),
                conversation_url: "https://chatgpt.com/c/synthetic-conversation".into(),
                user_turn_id: "synthetic-user-turn".into(),
                observed_model: "synthetic-model".into(),
            },
            now,
        )
        .await
        .unwrap();
    let markdown = "# Partial\n\n<script>unsafe()</script>\n\n```rust\nfn main() {}\n```\n";
    let run = store
        .capture(
            lease,
            Capture {
                markdown: markdown.into(),
                summary: None,
                evaluations: None,
                conversation_id: "synthetic-conversation".into(),
                conversation_url: "https://chatgpt.com/c/synthetic-conversation".into(),
                observed_model: "synthetic-model".into(),
                complete: false,
                error_code: Some("stream-interrupted".into()),
            },
            now + 1,
        )
        .await
        .unwrap();
    let artifact = &run.artifacts[0];
    let path = format!("/api/runs/{id}/artifacts/{}", artifact.id);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&path)
                .header("host", "localhost")
                .header("authorization", "Bearer synthetic-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "text/plain; charset=utf-8"
    );
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert_eq!(
        response.headers()["content-security-policy"],
        "default-src 'none'; sandbox"
    );
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(
        axum::body::to_bytes(response.into_body(), 10000)
            .await
            .unwrap(),
        markdown
    );
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("/api/runs/{id}/artifacts/unregistered"),
            Value::Null
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("/api/runs/different/artifacts/{}", artifact.id),
            Value::Null
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    std::fs::write(
        temp.path().join("runtime/artifacts").join(&artifact.id),
        "changed",
    )
    .unwrap();
    let tampered = call(&app, "GET", &path, Value::Null).await;
    assert_eq!(tampered.0, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(tampered.1["code"], "artifact-integrity");
    store.close().await.unwrap();
}

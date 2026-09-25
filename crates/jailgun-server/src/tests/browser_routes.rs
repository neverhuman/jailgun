use super::*;

#[tokio::test]
async fn lists_browser_accounts_from_registry() {
    let temp = tempfile::tempdir().unwrap();
    let registry_path = temp.path().join("browser-profiles.json");
    let roots = jailgun_core::BrowserAccountRoots {
        profile_root: temp.path().join("profiles"),
        state_root: temp.path().join("state"),
        downloads_root: temp.path().join("downloads"),
    };
    let mut registry = BrowserProfileRegistry::default();
    let account = registry
        .upsert_account(
            "user@example.com",
            Some("acct-test".into()),
            &roots,
            9229,
            3,
        )
        .unwrap();
    registry.save(&registry_path).unwrap();

    let (mut state, _rx) =
        AppState::live(JailgunConfig::default(), temp.path().join("receipts"), 64);
    state.browser_registry_path = registry_path;
    let state = state.with_ingest_token(Some("secret".into()));
    let app = api_router(state.clone());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/browser/accounts")
                .header("x-jailgun-token", "secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["accounts"][0]["id"], account.id);
    assert_eq!(value["accounts"][0]["cdp_url"], "http://127.0.0.1:9229");
}

#[tokio::test]
async fn browser_aliases_and_auth_status_share_the_same_backend() {
    let temp = tempfile::tempdir().unwrap();
    let registry_path = temp.path().join("browser-profiles.json");
    let roots = jailgun_core::BrowserAccountRoots {
        profile_root: temp.path().join("profiles"),
        state_root: temp.path().join("state"),
        downloads_root: temp.path().join("downloads"),
    };
    let mut registry = BrowserProfileRegistry::default();
    let account = registry
        .upsert_account(
            "user@example.com",
            Some("acct-test".into()),
            &roots,
            9229,
            3,
        )
        .unwrap();
    registry.accounts[0].status = BrowserAccountStatus::Ready;
    registry.save(&registry_path).unwrap();
    std::fs::write(temp.path().join("prompt.txt"), "prompt").unwrap();

    let (mut state, _rx) =
        AppState::live(JailgunConfig::default(), temp.path().join("receipts"), 64);
    state.browser_registry_path = registry_path;
    state.runs.write().await.push(RunSnapshot::fixture());
    let state = state.clone().with_ingest_token(Some("secret".into()));
    let app = api_router(state.clone());

    for uri in [
        "/api/browsers",
        "/api/browsers/acct-test",
        "/api/browsers/acct-test/auth/status",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header("x-jailgun-token", "secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        let account_json = if uri.ends_with("/acct-test") || uri.ends_with("/auth/status") {
            value
        } else {
            value["accounts"][0].clone()
        };
        assert_eq!(account_json["id"], account.id);
        assert_eq!(account_json["status"], "ready");
    }
}

#[tokio::test]
async fn auth_code_rejects_wrong_state_and_closed_session() {
    let temp = tempfile::tempdir().unwrap();
    let registry_path = temp.path().join("browser-profiles.json");
    let roots = jailgun_core::BrowserAccountRoots {
        profile_root: temp.path().join("profiles"),
        state_root: temp.path().join("state"),
        downloads_root: temp.path().join("downloads"),
    };
    let mut registry = BrowserProfileRegistry::default();
    let account = registry
        .upsert_account(
            "user@example.com",
            Some("acct-test".into()),
            &roots,
            9229,
            3,
        )
        .unwrap();
    registry.save(&registry_path).unwrap();

    let (mut state, _rx) =
        AppState::live(JailgunConfig::default(), temp.path().join("receipts"), 64);
    state.browser_registry_path = registry_path;
    let state = state.clone().with_ingest_token(Some("secret".into()));
    let app = api_router(state.clone());

    let not_started = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/browsers/acct-test/auth/code")
                .header("content-type", "application/json")
                .header("x-jailgun-token", "secret")
                .body(Body::from(r#"{"code":"123456"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(not_started.status(), StatusCode::CONFLICT);

    let (commands_tx, commands_rx) =
        mpsc::channel::<jailgun_orchestrator::bridge::Envelope<serde_json::Value>>(1);
    drop(commands_rx);
    state.browser_auth_sessions.write().await.insert(
        account.id.clone(),
        BrowserAuthSession {
            session_id: uuid::Uuid::new_v4(),
            commands_tx,
        },
    );

    let closed = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/browsers/acct-test/auth/code")
                .header("content-type", "application/json")
                .header("x-jailgun-token", "secret")
                .body(Body::from(r#"{"code":"123456"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(closed.status(), StatusCode::CONFLICT);
}

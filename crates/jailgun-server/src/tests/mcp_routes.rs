use super::*;

#[tokio::test]
async fn mcp_tools_call_routes_known_and_unknown_tools() {
    let temp = tempfile::tempdir().unwrap();
    let registry_path = temp.path().join("browser-profiles.json");
    let roots = jailgun_core::BrowserAccountRoots {
        profile_root: temp.path().join("profiles"),
        state_root: temp.path().join("state"),
        downloads_root: temp.path().join("downloads"),
    };
    let mut registry = BrowserProfileRegistry::default();
    let _account = registry
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

    let backend = Arc::new(FakeBackend);
    let (mut state, _rx) =
        AppState::live(JailgunConfig::default(), temp.path().join("receipts"), 64);
    state.browser_registry_path = registry_path;
    let state = state
        .with_ingest_token(Some("secret".into()))
        .with_agent_backend(backend);
    let app = api_router(state.clone());
    let prompt_file = temp.path().join("prompt.txt");
    std::fs::write(&prompt_file, "prompt").unwrap();

    let tools = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "method": "tools/list",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tools.status(), StatusCode::OK);
    let tools_body = axum::body::to_bytes(tools.into_body(), usize::MAX)
        .await
        .unwrap();
    let tools_value: Value = serde_json::from_slice(&tools_body).unwrap();
    assert!(tools_value["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "jailgun.run"));

    let unknown = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 2,
                        "method": "tools/call",
                        "params": {
                            "name": "nope",
                            "arguments": {},
                        },
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unknown.status(), StatusCode::OK);
    let unknown_body = axum::body::to_bytes(unknown.into_body(), usize::MAX)
        .await
        .unwrap();
    let unknown_value: Value = serde_json::from_slice(&unknown_body).unwrap();
    assert_eq!(unknown_value["error"]["code"], -32601);

    let auth_status = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("content-type", "application/json")
                .header("x-jailgun-token", "secret")
                .body(Body::from(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 3,
                        "method": "tools/call",
                        "params": {
                            "name": "jailgun.auth_status",
                            "arguments": { "account_id": "acct-test" },
                        },
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(auth_status.status(), StatusCode::OK);
    let auth_status_body = axum::body::to_bytes(auth_status.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_status_value: Value = serde_json::from_slice(&auth_status_body).unwrap();
    assert!(auth_status_value["result"]["content"].is_array());
    assert!(!auth_status_value["result"]["isError"]
        .as_bool()
        .unwrap_or(false));

    let accepted_run = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("content-type", "application/json")
                .header("x-jailgun-token", "secret")
                .body(Body::from(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 4,
                        "method": "tools/call",
                        "params": {
                            "name": "jailgun.run",
                            "arguments": {
                                "run_id": "mcp-account-alias",
                                "prompt_ref": "jmcp://prompt/1",
                                "prompt_file": prompt_file,
                                "tabs": 1,
                                "account": "acct-test",
                                "browser": {
                                    "bridge_cmd": ["fake-bridge"]
                                }
                            },
                        },
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(accepted_run.status(), StatusCode::OK);
    let accepted_body = axum::body::to_bytes(accepted_run.into_body(), usize::MAX)
        .await
        .unwrap();
    let accepted_value: Value = serde_json::from_slice(&accepted_body).unwrap();
    assert!(!accepted_value["result"]["isError"]
        .as_bool()
        .unwrap_or(false));
    assert_eq!(
        accepted_value["result"]["structuredContent"]["run_id"],
        "mcp-account-alias"
    );

    let rejected_run = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("content-type", "application/json")
                .header("x-jailgun-token", "secret")
                .body(Body::from(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 5,
                        "method": "tools/call",
                        "params": {
                            "name": "jailgun.run",
                            "arguments": {
                                "prompt_ref": "jmcp://prompt/1",
                                "prompt_file": temp.path().join("prompt.txt"),
                                "browser": {}
                            },
                        },
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected_run.status(), StatusCode::OK);
    let rejected_body = axum::body::to_bytes(rejected_run.into_body(), usize::MAX)
        .await
        .unwrap();
    let rejected_value: Value = serde_json::from_slice(&rejected_body).unwrap();
    assert!(rejected_value["result"]["isError"]
        .as_bool()
        .unwrap_or(false));
}

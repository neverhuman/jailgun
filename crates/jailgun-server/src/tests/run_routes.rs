use super::*;

#[tokio::test]
async fn start_run_accepts_request_and_publishes_snapshot() {
    let backend = Arc::new(FakeBackend);
    let receipt_dir = tempfile::tempdir().unwrap();
    let (state, _rx) = AppState::live(
        JailgunConfig::default(),
        receipt_dir.path().to_path_buf(),
        64,
    );
    let state = state
        .with_ingest_token(Some("secret".into()))
        .with_agent_backend(backend.clone());
    let app = api_router(state);
    let prompt_file =
        std::env::temp_dir().join(format!("jailgun-prompt-{}.txt", uuid::Uuid::new_v4()));
    std::fs::write(&prompt_file, "review this change").unwrap();

    let mut request = JailgunAgentRunRequest {
        version: jailgun_core::JAILGUN_AGENT_INTERFACE_VERSION,
        run_id: Some("run-1".into()),
        prompt_ref: "local://prompt/1".into(),
        prompt_file,
        config_path: None,
        tabs: Some(1),
        max_runtime_seconds: Some(60),
        repo: Default::default(),
        source_archive: Default::default(),
        deploy: Default::default(),
        ci: Default::default(),
        browser: Default::default(),
        github: Default::default(),
    };
    request.browser.bridge_cmd = vec!["fake-bridge".into()];
    request.browser.profile_dir = Some(std::env::temp_dir().join("jailgun-test-profile"));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/runs")
                .header("content-type", "application/json")
                .header("x-jailgun-token", "secret")
                .body(Body::from(serde_json::to_vec(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "agent run request was rejected: {}",
        String::from_utf8_lossy(&body)
    );

    let accepted: JailgunAgentRunAcceptedResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(accepted.run_id, "run-1");

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/api/runs/run-1/agent-summary")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
                .status()
                == StatusCode::OK
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("summary became available");
}

#[tokio::test]
async fn start_run_accepts_missing_v1_and_rejects_bad_version() {
    let backend = Arc::new(FakeBackend);
    let receipt_dir = tempfile::tempdir().unwrap();
    let (state, _rx) = AppState::live(
        JailgunConfig::default(),
        receipt_dir.path().to_path_buf(),
        64,
    );
    let app = api_router(
        state
            .with_ingest_token(Some("secret".into()))
            .with_agent_backend(backend),
    );
    let prompt_file =
        std::env::temp_dir().join(format!("jailgun-prompt-{}.txt", uuid::Uuid::new_v4()));
    std::fs::write(&prompt_file, "review this change").unwrap();

    let missing_version = json!({
        "run_id": "missing-version-ok",
        "prompt_ref": "local://prompt/1",
        "prompt_file": prompt_file,
        "tabs": 1,
        "browser": {
            "bridge_cmd": ["fake-bridge"],
            "profile_dir": std::env::temp_dir().join("jailgun-test-profile")
        }
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/runs")
                .header("content-type", "application/json")
                .header("x-jailgun-token", "secret")
                .body(Body::from(serde_json::to_vec(&missing_version).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let bad_version = json!({
        "version": 2,
        "prompt_ref": "local://prompt/1",
        "prompt_file": prompt_file
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/runs")
                .header("content-type", "application/json")
                .header("x-jailgun-token", "secret")
                .body(Body::from(serde_json::to_vec(&bad_version).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn start_run_rejects_path_like_run_id() {
    let receipt_dir = tempfile::tempdir().unwrap();
    let (state, _rx) = AppState::live(
        JailgunConfig::default(),
        receipt_dir.path().to_path_buf(),
        64,
    );
    let app = api_router(state.with_ingest_token(Some("secret".into())));
    let prompt_file =
        std::env::temp_dir().join(format!("jailgun-prompt-{}.txt", uuid::Uuid::new_v4()));
    std::fs::write(&prompt_file, "review this change").unwrap();
    let request = json!({
        "version": jailgun_core::JAILGUN_AGENT_INTERFACE_VERSION,
        "run_id": "../outside",
        "prompt_ref": "local://prompt/1",
        "prompt_file": prompt_file,
        "browser": {
            "bridge_cmd": ["fake-bridge"],
            "profile_dir": std::env::temp_dir().join("jailgun-test-profile")
        }
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/runs")
                .header("content-type", "application/json")
                .header("x-jailgun-token", "secret")
                .body(Body::from(serde_json::to_vec(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rest_run_accepts_canonical_browser_account_ids() {
    let temp = tempfile::tempdir().unwrap();
    let registry_path =
        write_browser_registry(temp.path(), &[("acct-test", BrowserAccountStatus::Ready)]);
    let prompt_file = write_prompt(temp.path(), "prompt.txt");
    let app = live_test_app(&temp, registry_path);

    let (status, value) = post_run(
        app,
        json!({
            "version": jailgun_core::JAILGUN_AGENT_INTERFACE_VERSION,
            "run_id": "canonical-account",
            "prompt_ref": "jmcp://prompt/1",
            "prompt_file": prompt_file,
            "tabs": 1,
            "browser": {
                "bridge_cmd": ["fake-bridge"],
                "account_ids": ["acct-test"]
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::ACCEPTED, "{value}");
    assert_eq!(value["run_id"], "canonical-account");
}

#[tokio::test]
async fn rest_run_normalizes_top_level_account_ids_alias() {
    let temp = tempfile::tempdir().unwrap();
    let registry_path =
        write_browser_registry(temp.path(), &[("acct-test", BrowserAccountStatus::Ready)]);
    let prompt_file = write_prompt(temp.path(), "prompt.txt");
    let app = live_test_app(&temp, registry_path);

    let (status, value) = post_run(
        app,
        json!({
            "version": jailgun_core::JAILGUN_AGENT_INTERFACE_VERSION,
            "run_id": "rest-account-alias",
            "prompt_ref": "jmcp://prompt/1",
            "prompt_file": prompt_file,
            "tabs": 1,
            "account_ids": ["acct-test"],
            "browser": {
                "bridge_cmd": ["fake-bridge"]
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::ACCEPTED, "{value}");
    assert_eq!(value["run_id"], "rest-account-alias");
}

#[tokio::test]
async fn run_rejects_conflicting_account_aliases() {
    let temp = tempfile::tempdir().unwrap();
    let registry_path = write_browser_registry(
        temp.path(),
        &[
            ("acct-a", BrowserAccountStatus::Ready),
            ("acct-b", BrowserAccountStatus::Ready),
        ],
    );
    let prompt_file = write_prompt(temp.path(), "prompt.txt");
    let app = live_test_app(&temp, registry_path);

    let (status, value) = post_run(
        app,
        json!({
            "version": jailgun_core::JAILGUN_AGENT_INTERFACE_VERSION,
            "run_id": "conflicting-account-alias",
            "prompt_ref": "jmcp://prompt/1",
            "prompt_file": prompt_file,
            "tabs": 1,
            "account_ids": ["acct-b"],
            "browser": {
                "bridge_cmd": ["fake-bridge"],
                "account_ids": ["acct-a"]
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(value["reason"]
        .as_str()
        .unwrap_or_default()
        .contains("conflicting browser account routing"));
}

#[tokio::test]
async fn run_rejects_duplicate_account_ids_at_ingress() {
    let temp = tempfile::tempdir().unwrap();
    let registry_path =
        write_browser_registry(temp.path(), &[("acct-test", BrowserAccountStatus::Ready)]);
    let prompt_file = write_prompt(temp.path(), "prompt.txt");
    let app = live_test_app(&temp, registry_path);

    let (status, value) = post_run(
        app,
        json!({
            "version": jailgun_core::JAILGUN_AGENT_INTERFACE_VERSION,
            "run_id": "duplicate-account",
            "prompt_ref": "jmcp://prompt/1",
            "prompt_file": prompt_file,
            "tabs": 1,
            "browser": {
                "bridge_cmd": ["fake-bridge"],
                "account_ids": ["acct-test", "acct-test"]
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(value["reason"]
        .as_str()
        .unwrap_or_default()
        .contains("duplicate browser account id"));
}

#[tokio::test]
async fn run_rejects_unknown_and_non_ready_accounts() {
    for (account_id, status) in [
        ("missing-account", BrowserAccountStatus::Ready),
        ("acct-auth", BrowserAccountStatus::AuthRequired),
        ("acct-locked", BrowserAccountStatus::Locked),
        ("acct-degraded", BrowserAccountStatus::Degraded),
        ("acct-manual", BrowserAccountStatus::ManualBrowserRequired),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let registry_accounts = if account_id == "missing-account" {
            vec![("acct-ready", BrowserAccountStatus::Ready)]
        } else {
            vec![(account_id, status)]
        };
        let registry_path = write_browser_registry(temp.path(), &registry_accounts);
        let prompt_file = write_prompt(temp.path(), "prompt.txt");
        let app = live_test_app(&temp, registry_path);

        let (response_status, value) = post_run(
            app,
            json!({
                "version": jailgun_core::JAILGUN_AGENT_INTERFACE_VERSION,
                "run_id": format!("route-{account_id}"),
                "prompt_ref": "jmcp://prompt/1",
                "prompt_file": prompt_file,
                "tabs": 1,
                "browser": {
                    "bridge_cmd": ["fake-bridge"],
                    "account_ids": [account_id]
                }
            }),
        )
        .await;

        assert_eq!(response_status, StatusCode::BAD_REQUEST, "{account_id}");
        let reason = value["reason"].as_str().unwrap_or_default();
        if account_id == "missing-account" {
            assert!(reason.contains("not registered"), "{reason}");
        } else {
            assert!(reason.contains("not ready"), "{reason}");
        }
    }
}

#[tokio::test]
async fn rejects_path_like_run_id_on_summary_and_receipts() {
    let app = api_router(AppState::fixture(JailgunConfig::default()));

    for uri in [
        "/api/runs/..%2Foutside/agent-summary",
        "/api/receipts/..%2Foutside",
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

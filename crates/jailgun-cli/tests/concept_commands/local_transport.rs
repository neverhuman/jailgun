use super::*;
use axum::{
    http::{HeaderMap, StatusCode},
    Json, Router,
};
use rmcp::{
    model::{CallToolRequestParams, ClientConfig, ProtocolVersion},
    transport::TokioChildProcess,
    ServiceExt,
};
use std::{
    process::Stdio,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

#[tokio::test]
async fn cli_and_stdio_mcp_keep_local_credentials_out_of_environment_proxies() {
    let fixture = Fixture::new().await;
    let requests = Arc::new(AtomicUsize::new(0));
    let credentials = Arc::new(AtomicUsize::new(0));
    let (seen, secrets) = (requests.clone(), credentials.clone());
    let proxy = Router::new().route(
        "/{*path}",
        axum::routing::any(move |headers: HeaderMap| {
            let (seen, secrets) = (seen.clone(), secrets.clone());
            async move {
                seen.fetch_add(1, Ordering::SeqCst);
                if headers.contains_key("authorization") {
                    secrets.fetch_add(1, Ordering::SeqCst);
                }
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(serde_json::json!({"code":"synthetic-proxy-used"})),
                )
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, proxy).await.unwrap();
    });
    let command = |args: &[&str]| {
        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_jailgun"));
        child
            .args(args)
            .env_clear()
            .env("HOME", fixture.dir.path())
            .env("JAILGUN_URL", &fixture.url)
            .env("JAILGUN_TOKEN", TOKEN)
            .env("NO_PROXY", "")
            .env("no_proxy", "")
            .current_dir(fixture.dir.path())
            .kill_on_drop(true);
        for key in [
            "HTTP_PROXY",
            "http_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "ALL_PROXY",
            "all_proxy",
        ] {
            child.env(key, &proxy_url);
        }
        child
    };
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        command(&["accounts", "list", "--json"]).output(),
    )
    .await
    .unwrap()
    .unwrap();
    let cli_requests = requests.load(Ordering::SeqCst);
    // Inspect counts before parsing output so a credential is never printed on failure.
    assert_eq!(
        cli_requests, 0,
        "CLI sent a loopback request to the environment proxy"
    );
    assert_eq!(success(output).as_array().unwrap().len(), 1);
    let (transport, _) = TokioChildProcess::builder(command(&["mcp"]))
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let connected = tokio::time::timeout(
        Duration::from_secs(15),
        ClientConfig::default()
            .with_protocol_version(ProtocolVersion::V_2025_11_25)
            .serve(transport),
    )
    .await
    .unwrap();
    assert_eq!(
        requests.load(Ordering::SeqCst),
        0,
        "MCP sent a loopback request to the environment proxy"
    );
    assert!(
        connected.is_ok(),
        "the official SDK could not connect to the local gateway"
    );
    let client = connected.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        let tools = client.list_all_tools().await.unwrap();
        assert!(tools.iter().any(|tool| tool.name == "jailgun.accounts"));
        let accounts = client
            .call_tool(CallToolRequestParams::new("jailgun.accounts"))
            .await
            .unwrap();
        assert_ne!(accounts.is_error, Some(true));
        assert_eq!(
            accounts.structured_content.unwrap()["accounts"][0]["id"],
            "synthetic"
        );
    })
    .await
    .expect("bounded SDK tool calls");
    tokio::time::timeout(Duration::from_secs(5), client.cancel())
        .await
        .expect("bounded SDK shutdown")
        .unwrap();
    assert_eq!(requests.load(Ordering::SeqCst), 0);
    assert_eq!(credentials.load(Ordering::SeqCst), 0);
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn stdio_mcp_rejects_non_owner_mutation_and_revoked_credentials() {
    let fixture = Fixture::new().await;
    let other_run = fixture.submit().await;
    fixture
        .store
        .connect_account(ConnectAccount {
            email: "second@example.invalid".into(),
            id: Some("second".into()),
        })
        .await
        .unwrap();
    let token = fixture
        .store
        .issue_token(
            IssueToken {
                name: "Scoped gateway proof".into(),
                account_ids: vec!["second".into()],
            },
            now(),
        )
        .await
        .unwrap();
    let command = |secret: &str| {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_jailgun"));
        command
            .arg("mcp")
            .env_clear()
            .env("HOME", fixture.dir.path())
            .env("JAILGUN_URL", &fixture.url)
            .env("JAILGUN_TOKEN", secret)
            .current_dir(fixture.dir.path())
            .kill_on_drop(true);
        command
    };
    let (transport, _) = TokioChildProcess::builder(command(&token.secret))
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let client = tokio::time::timeout(
        Duration::from_secs(10),
        ClientConfig::default().serve(transport),
    )
    .await
    .unwrap()
    .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        let tools = client.list_all_tools().await.unwrap();
        assert_eq!(tools.len(), 9);
        assert!(!tools.iter().any(|tool| tool.name == "jailgun.run"));
        for operation in ["jailgun.run_status", "jailgun.run_cancel"] {
            let denied = client
                .call_tool(
                    CallToolRequestParams::new(operation).with_arguments(
                        serde_json::json!({"run_id":other_run})
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await
                .unwrap();
            assert_eq!(denied.is_error, Some(true));
            assert_eq!(
                denied.structured_content.unwrap()["code"],
                "account-forbidden"
            );
        }
        assert_eq!(
            fixture.store.run(other_run.clone()).await.unwrap().status,
            "queued"
        );
        fixture
            .store
            .revoke_token(token.metadata.id, now())
            .await
            .unwrap();
        assert!(
            client.list_all_tools().await.is_err(),
            "revoked credentials still authorized a gateway request"
        );
    })
    .await
    .expect("bounded negative authorization proof");
    let _ = tokio::time::timeout(Duration::from_secs(5), client.cancel())
        .await
        .unwrap();
    let (transport, _) = TokioChildProcess::builder(command("incorrect-credential"))
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(
        tokio::time::timeout(
            Duration::from_secs(10),
            ClientConfig::default().serve(transport)
        )
        .await
        .unwrap()
        .is_err(),
        "invalid credential connected to the gateway"
    );
}

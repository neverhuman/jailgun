use super::*;
use jailgun_workflow::model::{ConnectAccount, IssueToken};
use rmcp::{
    model::{CallToolRequestParams, CallToolResult, ClientConfig, ProtocolVersion},
    service::RunningService,
    transport::{
        streamable_http_client::StreamableHttpClientTransportConfig, StreamableHttpClientTransport,
    },
    RoleClient, ServiceExt as McpServiceExt,
};
use std::time::Duration;

async fn listen(app: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, task)
}

async fn client(url: &str, secret: &str) -> RunningService<RoleClient, ClientConfig> {
    let mut config =
        StreamableHttpClientTransportConfig::with_uri(format!("{url}/mcp")).auth_header(secret);
    config.allow_stateless = true;
    let client = tokio::time::timeout(
        Duration::from_secs(5),
        ClientConfig::default()
            .with_protocol_version(ProtocolVersion::V_2025_11_25)
            .serve(StreamableHttpClientTransport::from_config(config)),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        client.peer_info().unwrap().protocol_version,
        ProtocolVersion::V_2025_11_25
    );
    client
}

async fn call(
    client: &RunningService<RoleClient, ClientConfig>,
    name: &'static str,
    args: Value,
) -> CallToolResult {
    tokio::time::timeout(
        Duration::from_secs(5),
        client.call_tool(
            CallToolRequestParams::new(name).with_arguments(args.as_object().unwrap().clone()),
        ),
    )
    .await
    .unwrap()
    .unwrap()
}

#[tokio::test]
async fn official_sdk_negotiates_and_enforces_scopes_and_revocation_per_request() {
    let (_directory, store, app) = super::concept_routes::fixture().await;
    store
        .connect_account(ConnectAccount {
            email: "second@example.invalid".into(),
            id: Some("second".into()),
        })
        .await
        .unwrap();
    let token = store
        .issue_token(
            IssueToken {
                name: "SDK client".into(),
                account_ids: vec!["synthetic".into()],
            },
            crate::concepts::now(),
        )
        .await
        .unwrap();
    let (url, server) = listen(app).await;
    let client = client(&url, &token.secret).await;
    let tools = client.list_all_tools().await.unwrap();
    assert_eq!(tools.len(), 9);
    assert!(!tools.iter().any(|t| t.name == "jailgun.run"
        || t.name == "jailgun.run_summary"
        || t.name == "jailgun.submit_code"));
    for tool in &tools {
        assert_eq!(tool.input_schema["type"], "object");
        assert!(tool.output_schema.is_some(), "{}", tool.name);
    }
    assert!(client
        .call_tool(CallToolRequestParams::new("unknown"))
        .await
        .is_err());
    assert!(client
        .call_tool(CallToolRequestParams::new("jailgun.run_status"))
        .await
        .is_err());
    let auth = call(&client, "jailgun.auth_status", json!({})).await;
    assert_eq!(
        auth.structured_content.unwrap()["dashboard_url"],
        format!("{url}/#accounts")
    );
    let accounts = call(&client, "jailgun.accounts", json!({})).await;
    assert_eq!(
        accounts.structured_content.unwrap()["accounts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let forbidden = call(
        &client,
        "jailgun.brainstorm",
        json!({"concept":"test","account_id":"second","idempotency_key":"no"}),
    )
    .await;
    assert_eq!(forbidden.is_error, Some(true));
    assert_eq!(
        forbidden.structured_content.unwrap()["code"],
        "account-forbidden"
    );
    let archive = call(
        &client,
        "jailgun.run",
        json!({"prompt_ref":"synthetic","prompt_file":"unused"}),
    )
    .await;
    assert_eq!(
        archive.structured_content.unwrap()["code"],
        "operator-required"
    );
    let args = json!({"concept":"A synthetic library","account_id":"synthetic","idempotency_key":"sdk-concept"});
    let accepted = call(&client, "jailgun.brainstorm", args.clone()).await;
    assert_ne!(accepted.is_error, Some(true));
    let accepted = accepted.structured_content.unwrap();
    assert_eq!(
        call(&client, "jailgun.brainstorm", args)
            .await
            .structured_content
            .unwrap(),
        accepted
    );
    let run_id = accepted["run_id"].as_str().unwrap();
    for (name, status) in [
        ("jailgun.run_pause", "paused"),
        ("jailgun.run_resume", "queued"),
        ("jailgun.run_cancel", "cancelled"),
        ("jailgun.run_cancel", "cancelled"),
    ] {
        assert_eq!(
            call(&client, name, json!({"run_id":run_id}))
                .await
                .structured_content
                .unwrap()["status"],
            status
        );
    }
    assert_eq!(
        call(&client, "jailgun.run_status", json!({"run_id":run_id}))
            .await
            .structured_content
            .unwrap()["status"],
        "cancelled"
    );
    assert_eq!(
        call(&client, "jailgun.run_result", json!({"run_id":run_id}))
            .await
            .structured_content
            .unwrap()["code"],
        "result-not-complete"
    );
    store
        .revoke_token(token.metadata.id, crate::concepts::now())
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), client.list_all_tools())
            .await
            .unwrap()
            .is_err()
    );
    let _ = client.cancel().await;
    server.abort();
    let _ = server.await;
    store.close().await.unwrap();
}

#[tokio::test]
async fn sdk_preserves_operator_archive_alias_and_status() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/mcp-proofs");
    std::fs::create_dir_all(&root).unwrap();
    let temp = tempfile::Builder::new().tempdir_in(root).unwrap();
    let registry =
        write_browser_registry(temp.path(), &[("acct-test", BrowserAccountStatus::Ready)]);
    let (url, server) = listen(live_test_app(&temp, registry)).await;
    let client = client(&url, "secret").await;
    assert_eq!(client.list_all_tools().await.unwrap().len(), 11);
    let accepted=call(&client,"jailgun.run",json!({"run_id":"mcp-account-alias","config_path":fixture_config(),"prompt_ref":"synthetic","prompt_file":write_prompt(temp.path(),"prompt.txt"),"tabs":1,"account":"acct-test","browser":{"bridge_cmd":["fake-bridge"]}})).await;
    assert_ne!(accepted.is_error, Some(true), "{accepted:?}");
    assert_eq!(
        accepted.structured_content.unwrap()["run_id"],
        "mcp-account-alias"
    );
    let status = call(
        &client,
        "jailgun.run_status",
        json!({"run_id":"mcp-account-alias"}),
    )
    .await;
    assert_ne!(status.is_error, Some(true));
    let summary = call(
        &client,
        "jailgun.run_summary",
        json!({"run_id":"mcp-account-alias"}),
    )
    .await;
    assert_ne!(summary.is_error, Some(true));
    let rejected = call(
        &client,
        "jailgun.run",
        json!({"prompt_ref":"synthetic","prompt_file":temp.path().join("prompt.txt"),"browser":{}}),
    )
    .await;
    assert_eq!(rejected.is_error, Some(true));
    client.cancel().await.unwrap();
    server.abort();
    let _ = server.await;
}

struct McpMockWorkerExecutor;

#[async_trait::async_trait]
impl crate::worker::WorkerExecutor for McpMockWorkerExecutor {
    fn name(&self) -> &'static str {
        "mock-ci"
    }

    async fn execute(
        &self,
        request: crate::worker::ExecutionRequest,
        _cancel: tokio::sync::watch::Receiver<bool>,
    ) -> std::result::Result<crate::worker::ExecutionResult, crate::worker::ExecutionFailure> {
        std::fs::write(
            request.workspace.join("jailgun-result.md"),
            "# Synthetic worker result\n",
        )
        .map_err(|error| crate::worker::ExecutionFailure {
            kind: crate::worker::ExecutionFailureKind::Failed,
            message: error.to_string(),
        })?;
        Ok(crate::worker::ExecutionResult { exit_code: 0 })
    }
}

#[tokio::test]
async fn mcp_worker_mock_covers_tabs_models_jobs_status_and_output_objects() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/worker-mcp-proofs");
    std::fs::create_dir_all(&root).unwrap();
    let temp = tempfile::Builder::new().tempdir_in(root).unwrap();
    let worker =
        crate::worker::WorkerService::new(temp.path(), Arc::new(McpMockWorkerExecutor)).unwrap();
    let (state, _events) =
        AppState::live(JailgunConfig::default(), temp.path().join("receipts"), 64);
    let (url, server) = listen(api_router(
        state
            .with_ingest_token(Some("worker-secret".into()))
            .with_worker(worker),
    ))
    .await;
    let client = client(&url, "worker-secret").await;
    let tools = client.list_all_tools().await.unwrap();
    assert_eq!(tools.len(), 14);
    assert!(tools
        .iter()
        .all(|tool| tool.name.starts_with("jailgun.worker.")));
    let info = call(&client, "jailgun.worker.info", json!({})).await;
    assert_eq!(info.structured_content.unwrap()["executor"], "mock-ci");
    let account = call(
        &client,
        "jailgun.worker.account_register",
        json!({"account_id":"ci-primary","label":"CI primary"}),
    )
    .await;
    assert_eq!(account.structured_content.unwrap()["status"], "ready");
    let accounts = call(
        &client,
        "jailgun.worker.account_list",
        json!({"refresh":true}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(accounts["counts"]["total"], 1);
    assert_eq!(accounts["counts"]["ready"], 1);
    let tab = call(
        &client,
        "jailgun.worker.tab_open",
        json!({"tab_id":"ci-tab","account_id":"ci-primary","model":"sol","reasoning_effort":"high"}),
    )
    .await;
    let tab = tab.structured_content.unwrap();
    assert_eq!(tab["model"], "gpt-5.6-sol");
    let job = call(
        &client,
        "jailgun.worker.job_submit",
        json!({
            "tab_id":"ci-tab",
            "prompt":"Produce a synthetic result without a live model.",
            "model":"astra",
            "reasoning_effort":"ultra",
            "timeout_seconds":30,
            "error_reporting":"detailed",
            "idempotency_key":"worker-mcp-ci-1"
        }),
    )
    .await;
    let job_id = job.structured_content.unwrap()["job_id"]
        .as_str()
        .unwrap()
        .to_string();
    let completed = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let status = call(
                &client,
                "jailgun.worker.job_status",
                json!({"job_id":job_id}),
            )
            .await
            .structured_content
            .unwrap();
            if status["status"] == "completed" {
                break status;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(completed["model"], "gpt-6-astra");
    assert_eq!(completed["reasoning_effort"], "ultra");
    assert!(completed["output_object_id"].as_str().is_some());
    let objects = call(&client, "jailgun.worker.object_list", json!({})).await;
    assert_eq!(
        objects.structured_content.unwrap()["objects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    client.cancel().await.unwrap();
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn mcp_rejects_origin_and_handles_notifications_and_protocol_errors() {
    let (_directory, store, app) = super::concept_routes::fixture().await;
    let (url, server) = listen(app).await;
    let client = reqwest::Client::new();
    let post = |body| {
        client
            .post(format!("{url}/mcp"))
            .bearer_auth("synthetic-token")
            .header("accept", "application/json, text/event-stream")
            .json(&body)
    };
    let notification = post(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
        .send()
        .await
        .unwrap();
    assert_eq!(notification.status(), StatusCode::ACCEPTED);
    assert!(notification.bytes().await.unwrap().is_empty());
    assert_eq!(
        post(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}))
            .header("origin", "https://attacker.invalid")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}))
            .header("mcp-protocol-version", "1900-01-01")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let error = post(json!({"jsonrpc":"2.0","id":4,"method":"unknown/method"}))
        .send()
        .await
        .unwrap();
    assert!(
        error.status().is_client_error()
            || error.json::<Value>().await.unwrap()["error"]["code"].is_number()
    );
    server.abort();
    let _ = server.await;
    store.close().await.unwrap();
}

#[tokio::test]
async fn sdk_reads_registered_partial_artifacts_and_revocation_blocks_next_chunk() {
    use jailgun_workflow::model::*;
    let (_directory, store, app) = super::concept_routes::fixture().await;
    let _run = store
        .submit(
            ConceptRequest {
                concept: "Synthetic chunk fixture".into(),
                account_id: "synthetic".into(),
                candidate_count: 5,
                constraints: String::new(),
                criteria: default_criteria(),
                idempotency_key: "artifact-sdk".into(),
            },
            ModelSelection::Current,
            0,
        )
        .await
        .unwrap();
    let lease = store.claim(0, 0).await.unwrap().unwrap();
    store.begin_submission(lease.clone(), 0).await.unwrap();
    let turn = AcceptedTurn {
        conversation_id: "fixture".into(),
        conversation_url: "https://chatgpt.com/c/fixture".into(),
        user_turn_id: "user-fixture".into(),
        observed_model: "synthetic".into(),
    };
    store
        .accepted(lease.clone(), turn.clone(), 1)
        .await
        .unwrap();
    let run = store
        .capture(
            lease,
            Capture {
                markdown: "😀 retained partial output".into(),
                summary: None,
                evaluations: None,
                conversation_id: turn.conversation_id,
                conversation_url: turn.conversation_url,
                observed_model: turn.observed_model,
                complete: false,
                error_code: Some("interrupted".into()),
            },
            2,
        )
        .await
        .unwrap();
    let artifact = &run.artifacts[0];
    let token = store
        .issue_token(
            IssueToken {
                name: "Reader".into(),
                account_ids: vec!["synthetic".into()],
            },
            3,
        )
        .await
        .unwrap();
    let (url, server) = listen(app).await;
    let client = client(&url, &token.secret).await;
    store
        .connect_account(ConnectAccount {
            email: "outsider@example.invalid".into(),
            id: Some("outsider".into()),
        })
        .await
        .unwrap();
    let other = store
        .issue_token(
            IssueToken {
                name: "Other reader".into(),
                account_ids: vec!["outsider".into()],
            },
            3,
        )
        .await
        .unwrap();
    let outsider = self::client(&url, &other.secret).await;
    let forbidden = call(
        &outsider,
        "jailgun.run_artifact",
        json!({"run_id":run.id,"artifact_id":artifact.id}),
    )
    .await;
    assert_eq!(
        forbidden.structured_content.unwrap()["code"],
        "account-forbidden"
    );
    let _ = outsider.cancel().await;
    let args = json!({"run_id":run.id,"artifact_id":artifact.id,"offset":0,"limit":4});
    let first = call(&client, "jailgun.run_artifact", args.clone()).await;
    assert_ne!(first.is_error, Some(true));
    let chunk = first.structured_content.unwrap();
    assert_eq!(chunk["text"], "😀");
    assert_eq!(chunk["next_offset"], 4);
    assert_eq!(chunk["artifact"]["completion"], "partial");
    assert_eq!(chunk["artifact"]["sha256"], artifact.sha256);
    let mut invalid = args.clone();
    invalid["artifact_id"] = json!("../../operator-token");
    assert_eq!(
        call(&client, "jailgun.run_artifact", invalid)
            .await
            .structured_content
            .unwrap()["code"],
        "not-found"
    );
    store.revoke_token(token.metadata.id, 4).await.unwrap();
    let mut next = args;
    next["offset"] = chunk["next_offset"].clone();
    assert!(client
        .call_tool(
            CallToolRequestParams::new("jailgun.run_artifact")
                .with_arguments(next.as_object().unwrap().clone())
        )
        .await
        .is_err());
    let _ = client.cancel().await;
    server.abort();
    let _ = server.await;
    store.close().await.unwrap();
}

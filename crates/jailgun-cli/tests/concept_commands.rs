#[path = "concept_commands/data.rs"]
mod data;
#[path = "concept_commands/local_transport.rs"]
mod local_transport;
mod support;
use jailgun_workflow::model::*;
use support::*;

#[tokio::test]
async fn file_submission_idempotency_controls_and_wait_failure_use_the_real_daemon() {
    let fixture = Fixture::new().await;
    let path = fixture.dir.path().join("concept with spaces.md");
    let text = "# A synthetic concept\n\nPreserve Unicode: café and exact newlines.\n";
    std::fs::write(&path, text).unwrap();
    let args = [
        "brainstorm",
        "--concept-file",
        path.to_str().unwrap(),
        "--account",
        "synthetic",
        "--tabs",
        "8",
        "--criterion",
        "Value=70",
        "--criterion",
        "Feasibility=30",
        "--idempotency-key",
        "file-key",
    ];
    let accepted = success(fixture.call(&args).await);
    assert_eq!(success(fixture.call(&args).await), accepted);
    let id = accepted["run_id"].as_str().unwrap();
    let run = success(
        fixture
            .call(&["runs", "show", id, "--url", &fixture.url])
            .await,
    );
    assert_eq!(run["request"]["concept"], text);
    assert_eq!(run["request"]["candidate_count"], 8);
    assert_eq!(run["request"]["criteria"][0]["weight"], 70);
    std::fs::write(&path, "Different content").unwrap();
    failure(fixture.call(&args).await, "idempotency-conflict");
    std::fs::write(&path, text).unwrap();
    assert_eq!(
        success(fixture.call(&["runs", "pause", id]).await)["status"],
        "paused"
    );
    failure(
        fixture
            .call(&["runs", "resume", id, "--allow-incomplete"])
            .await,
        "subset-unavailable",
    );
    assert_eq!(
        success(fixture.call(&["runs", "resume", id]).await)["status"],
        "queued"
    );
    assert_eq!(
        success(fixture.call(&["runs", "cancel", id]).await)["status"],
        "cancelled"
    );
    assert_eq!(
        success(fixture.call(&["runs", "cancel", id]).await)["status"],
        "cancelled"
    );
    let mut wait_args = args.to_vec();
    wait_args.push("--wait");
    failure(fixture.call(&wait_args).await, "run-not-complete");
    failure(
        fixture.call(&["runs", "result", id]).await,
        "result-not-complete",
    );
    assert_eq!(
        success(fixture.call(&["runs", "list"]).await)
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn partial_export_is_honest_private_and_never_overwrites() {
    let fixture = Fixture::new().await;
    let id = fixture.submit().await;
    let lease = fixture.store.claim(now(), 0).await.unwrap().unwrap();
    fixture
        .store
        .begin_submission(lease.clone(), now())
        .await
        .unwrap();
    fixture
        .store
        .accepted(
            lease.clone(),
            AcceptedTurn {
                conversation_id: "synthetic-conversation".into(),
                conversation_url: "https://chatgpt.com/c/synthetic-conversation".into(),
                user_turn_id: "synthetic-turn".into(),
                observed_model: "Fixture".into(),
            },
            now(),
        )
        .await
        .unwrap();
    let markdown = "# Partial response\n\n```rust\nfn example() {}\n```\n";
    let run = fixture
        .store
        .capture(
            lease,
            Capture {
                markdown: markdown.into(),
                summary: None,
                evaluations: None,
                conversation_id: "synthetic-conversation".into(),
                conversation_url: "https://chatgpt.com/c/synthetic-conversation".into(),
                observed_model: "Fixture".into(),
                complete: false,
                error_code: Some("stream-interrupted".into()),
            },
            now(),
        )
        .await
        .unwrap();
    let destination = fixture.dir.path().join("export with spaces");
    let args = [
        "runs",
        "export",
        &id,
        "--out",
        destination.to_str().unwrap(),
    ];
    success(fixture.call(&args).await);
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(destination.join("export.json")).unwrap()).unwrap();
    assert_eq!(manifest["files"][0]["completion"], "partial");
    let artifact = destination.join(manifest["files"][0]["path"].as_str().unwrap());
    assert_eq!(std::fs::read_to_string(&artifact).unwrap(), markdown);
    assert!(!destination.join("final.md").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&artifact).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    failure(fixture.call(&args).await, "export-directory-not-empty");
    assert_eq!(std::fs::read_to_string(&artifact).unwrap(), markdown);
    let original = fixture
        .dir
        .path()
        .join("runtime/artifacts")
        .join(&run.artifacts[0].id);
    std::fs::write(original, "tampered").unwrap();
    let bad = fixture.dir.path().join("tampered-export");
    failure(
        fixture
            .call(&["runs", "export", &id, "--out", bad.to_str().unwrap()])
            .await,
        "artifact-integrity",
    );
    assert!(!bad.join("export.json").exists());
}

#[tokio::test]
async fn credential_scope_and_pairing_are_enforced_for_cli_calls() {
    let fixture = Fixture::new().await;
    failure(
        invoke(
            fixture.dir.path(),
            &fixture.url,
            "incorrect",
            &["accounts", "list"],
        )
        .await,
        "unauthorized",
    );
    let token = fixture
        .store
        .issue_token(
            IssueToken {
                name: "CLI proof".into(),
                account_ids: vec!["synthetic".into()],
            },
            now(),
        )
        .await
        .unwrap();
    assert_eq!(
        success(
            invoke(
                fixture.dir.path(),
                &fixture.url,
                &token.secret,
                &["accounts", "list"]
            )
            .await
        )
        .as_array()
        .unwrap()
        .len(),
        1
    );
    failure(
        invoke(
            fixture.dir.path(),
            &fixture.url,
            &token.secret,
            &["setup", "--no-open"],
        )
        .await,
        "operator-required",
    );
    let setup = success(fixture.call(&["setup", "--no-open"]).await);
    assert_eq!(setup["browser_opened"], false);
    let link = reqwest::Url::parse(setup["dashboard_url"].as_str().unwrap()).unwrap();
    assert!(link.query().is_none());
    assert!(link.fragment().unwrap().starts_with("pair="));
    let code = link
        .fragment()
        .unwrap()
        .split('&')
        .next()
        .unwrap()
        .strip_prefix("pair=")
        .unwrap();
    let http = reqwest::Client::new();
    let pair = || {
        http.post(format!("{}/api/session/pair", fixture.url))
            .header("origin", &fixture.url)
            .json(&serde_json::json!({"code":code}))
            .send()
    };
    assert_eq!(pair().await.unwrap().status(), 204);
    assert_eq!(pair().await.unwrap().status(), 401);
}

#[tokio::test]
async fn probe_waits_for_a_starting_service_and_rejects_an_advanced_only_server() {
    use axum::{routing::get, Json, Router};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let dir = directory();
    let requests = Arc::new(AtomicUsize::new(0));
    let seen = requests.clone();
    let router = Router::new().route(
        "/api/accounts",
        get(move || {
            let seen = seen.clone();
            async move {
                if seen.fetch_add(1, Ordering::SeqCst) == 0 {
                    tokio::time::sleep(std::time::Duration::from_secs(4)).await;
                }
                Json(Vec::<AccountReadiness>::new())
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let result = invoke(dir.path(), &url, TOKEN, &["accounts", "list"]).await;
    server.abort();
    let _ = server.await;
    assert_eq!(success(result), serde_json::json!([]));
    assert!(requests.load(Ordering::SeqCst) >= 2);

    let router = jailgun_server::api_router(
        jailgun_server::AppState::fixture(jailgun_core::JailgunConfig::default())
            .with_ingest_token(Some(TOKEN.into())),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let result = invoke(dir.path(), &url, TOKEN, &["setup", "--no-open"]).await;
    server.abort();
    let _ = server.await;
    failure(result, "workflow-unavailable");
}

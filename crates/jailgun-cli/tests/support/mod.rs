use jailgun_core::JailgunConfig;
use jailgun_server::{api_router, AppState};
use jailgun_workflow::{model::*, Store};
use serde_json::Value;
use std::{path::Path, process::Output, time::Duration};

pub const TOKEN: &str = "synthetic-cli-operator-credential-000000001";
pub fn now() -> i64 {
    (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64
}
pub fn directory() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/cli-proofs");
    std::fs::create_dir_all(&root).unwrap();
    tempfile::Builder::new()
        .prefix("CLI with spaces ")
        .tempdir_in(root)
        .unwrap()
}
pub async fn invoke(root: &Path, url: &str, token: &str, args: &[&str]) -> Output {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_jailgun"));
    command
        .args(args)
        .arg("--json")
        .env_clear()
        .env("JAILGUN_URL", url)
        .env("JAILGUN_TOKEN", token)
        .env("HOME", root)
        .current_dir(root)
        .kill_on_drop(true);
    tokio::time::timeout(Duration::from_secs(15), command.output())
        .await
        .unwrap()
        .unwrap()
}
pub fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
pub fn failure(output: Output, code: &str) -> Value {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty(), "unexpected protocol stdout");
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], code, "{error}");
    assert!(error["next_action"].as_str().is_some_and(|s| !s.is_empty()));
    error
}
pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub store: Store,
    pub url: String,
    task: tokio::task::JoinHandle<std::io::Result<()>>,
}
impl Fixture {
    pub async fn new() -> Self {
        let dir = directory();
        let store = Store::open(dir.path().join("runtime")).unwrap();
        let account = store
            .connect_account(ConnectAccount {
                email: "synthetic@example.invalid".into(),
                id: Some("synthetic".into()),
            })
            .await
            .unwrap();
        let observation = AccountObservation {
            identity: AccountIdentity {
                id: "synthetic-provider-account".into(),
                email: account.email_hint,
            },
            model: "Fixture".into(),
            available_models: vec!["Fixture".into()],
        };
        store
            .login_state(
                account.id.clone(),
                "verifying".into(),
                None,
                Some(now() + 900_000),
            )
            .await
            .unwrap();
        store
            .observe_account(account.id.clone(), observation.clone(), now())
            .await
            .unwrap();
        store
            .confirm_account(
                account.id,
                ConfirmAccount {
                    identity: observation.identity,
                    model: ModelSelection::Current,
                },
                now(),
            )
            .await
            .unwrap();
        let app = api_router(
            AppState::fixture(JailgunConfig::default())
                .with_ingest_token(Some(TOKEN.into()))
                .with_workflow(store.clone()),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await });
        Self {
            dir,
            store,
            url,
            task,
        }
    }
    pub async fn call(&self, args: &[&str]) -> Output {
        invoke(self.dir.path(), &self.url, TOKEN, args).await
    }
    pub async fn submit(&self) -> String {
        success(
            self.call(&[
                "brainstorm",
                "Synthetic neighborhood library",
                "--account",
                "synthetic",
                "--idempotency-key",
                "cli-submission",
            ])
            .await,
        )["run_id"]
            .as_str()
            .unwrap()
            .into()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

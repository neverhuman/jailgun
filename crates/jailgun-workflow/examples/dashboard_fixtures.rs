//! Synthetic contract and dashboard fixtures produced by real storage transitions.
use jailgun_workflow::{model::*, Store};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/contract-fixtures");
    jailgun_core::browser_registry::ensure_private_dir(&output)?;
    let directory = tempfile::tempdir_in(output)?;
    let store = Store::open(directory.path())?;
    let account = store
        .connect_account(ConnectAccount {
            email: "demo@example.invalid".into(),
            id: Some("demo-account".into()),
        })
        .await?;
    let mut fixtures = BTreeMap::<String, Value>::new();
    for state in [
        "disconnected",
        "starting",
        "login-required",
        "waiting-for-user",
    ] {
        store
            .login_state(account.id.clone(), state.into(), None, Some(900_000))
            .await?;
        fixtures.insert(
            format!("account-{state}"),
            json!(store.account_sessions().await?[0]),
        );
    }
    let identity = AccountIdentity {
        id: "synthetic-demo-identity".into(),
        email: account.email_hint,
    };
    store
        .observe_account(
            account.id.clone(),
            AccountObservation {
                identity: identity.clone(),
                model: "Synthetic model".into(),
                available_models: vec!["Synthetic model".into(), "Synthetic reasoning".into()],
            },
            0,
        )
        .await?;
    store
        .login_state(account.id.clone(), "verifying".into(), None, Some(900_000))
        .await?;
    fixtures.insert(
        "account-verifying".into(),
        json!(store.account_sessions().await?[0]),
    );
    store
        .confirm_account(
            account.id.clone(),
            ConfirmAccount {
                identity,
                model: ModelSelection::Current,
            },
            1,
        )
        .await?;
    fixtures.insert(
        "account-ready".into(),
        json!(store.account_sessions().await?[0]),
    );
    let run = store
        .submit(
            ConceptRequest {
                concept: "A neighborhood tool library with a small volunteer team".into(),
                account_id: account.id.clone(),
                candidate_count: 5,
                constraints:
                    "Use existing community space. Test demand before purchasing equipment.".into(),
                criteria: default_criteria(),
                idempotency_key: "synthetic-demo".into(),
            },
            ModelSelection::Current,
            2,
        )
        .await?;
    fixtures.insert("run-queued".into(), json!(run));
    for index in 0..5 {
        let now = 10 + index * 35_000;
        let lease = store.claim(now, 0).await?.expect("candidate claim");
        store.begin_submission(lease.clone(), now).await?;
        store
            .accepted(
                lease.clone(),
                AcceptedTurn {
                    conversation_id: format!("synthetic-{}", lease.position),
                    conversation_url: format!(
                        "https://chatgpt.com/c/synthetic-{}",
                        if lease.stage == "explore" {
                            lease.position.to_string()
                        } else {
                            lease.stage.clone()
                        }
                    ),
                    user_turn_id: format!("synthetic-user-{}", lease.position),
                    observed_model: "Synthetic model".into(),
                },
                now + 1,
            )
            .await?;
        if index == 0 {
            fixtures.insert(
                "run-running".into(),
                json!(store.run(run.id.clone()).await?),
            );
        }
        let mut captured = capture(&lease, 5);
        if index >= 3 {
            captured.complete = false;
            captured.error_code = Some("stream-interrupted".into());
        }
        store.capture(lease, captured, now + 100).await?;
    }
    assert!(store.claim(190_000, 0).await?.is_none());
    fixtures.insert(
        "run-partial".into(),
        json!(store.run(run.id.clone()).await?),
    );
    fixtures.insert(
        "partial-attempts".into(),
        json!(store.attempts(run.id.clone()).await?),
    );
    store.resume(run.id.clone(), true, 200_000).await?;
    fixtures.insert("run-subset".into(), json!(store.run(run.id.clone()).await?));
    for index in 0..4 {
        let now = 200_000 + index * 35_000;
        let lease = store.claim(now, 0).await?.expect("reduction claim");
        store.begin_submission(lease.clone(), now).await?;
        store
            .accepted(
                lease.clone(),
                AcceptedTurn {
                    conversation_id: format!("synthetic-{}", lease.stage),
                    conversation_url: format!(
                        "https://chatgpt.com/c/synthetic-{}",
                        if lease.stage == "explore" {
                            lease.position.to_string()
                        } else {
                            lease.stage.clone()
                        }
                    ),
                    user_turn_id: format!("synthetic-user-{}", lease.stage),
                    observed_model: "Synthetic model".into(),
                },
                now + 1,
            )
            .await?;
        store
            .capture(lease.clone(), capture(&lease, 3), now + 100)
            .await?;
        fixtures.insert(
            format!("after-{}", lease.stage),
            json!(store.run(run.id.clone()).await?),
        );
    }
    let completed = store.run(run.id.clone()).await?;
    let mut artifacts = BTreeMap::new();
    for artifact in &completed.artifacts {
        let (_, bytes) = store.artifact(run.id.clone(), artifact.id.clone()).await?;
        artifacts.insert(artifact.id.clone(), String::from_utf8(bytes)?);
    }
    fixtures.insert("run-completed".into(), json!(completed));
    fixtures.insert(
        "completed-attempts".into(),
        json!(store.attempts(run.id.clone()).await?),
    );
    fixtures.insert(
        "events".into(),
        json!(store.events(run.id.clone(), 0).await?),
    );
    fixtures.insert("artifacts".into(), json!(artifacts));
    fixtures.insert("accounts".into(), json!(store.accounts().await?));
    store.close().await?;
    println!("{}", serde_json::to_string(&fixtures)?);
    Ok(())
}

fn capture(lease: &Lease, count: u16) -> Capture {
    let proposal = "Pilot a shared tool shelf at the community center with a reservation notebook and a weekly volunteer shift.";
    let markdown = match lease.stage.as_str() {
        "revise" => format!("# A small tool library, tested first\n\n{proposal}\n\n## Why this proposal\n\nA small pilot tests demand and maintenance effort before committing to a larger collection.\n\n## Alternatives considered\n\n- A mobile collection reaches more people but requires transport.\n- A dedicated premises offers more space but increases fixed costs.\n\n## Tradeoffs and assumptions\n\nThis needs dependable volunteers and secure storage. Demand and replacement costs remain unverified. Start with hand tools and exclude equipment requiring specialist training.\n\n## Next steps\n\n1. Interview ten neighbors about tools they would borrow.\n2. Agree on storage, opening times and a replacement budget.\n3. Run a six-week pilot and review borrowing and maintenance records.\n\n*This is synthetic demonstration content, not a live model response.*"),
        "critique" => "# Critique\n\nVolunteer availability and maintenance costs are untested. Limit the initial inventory, define a replacement budget, and record unmet demand before expanding.".into(),
        _ => format!("# {} perspective\n\n{proposal}\n\n- Test demand first.\n- Track maintenance work.\n\n```text\nPilot → measure → review\n```\n\n[Illustrative reference](https://example.invalid/tool-library)\n\nThis content is synthetic.", lease.stage),
    };
    Capture {
        markdown,
        summary: (lease.stage == "explore").then(|| CandidateSummary {
            proposal: proposal.into(),
            strengths: vec!["Low initial cost".into()],
            weaknesses: vec!["Limited opening hours".into()],
            assumptions: vec!["Volunteer availability needs testing".into()],
            next_steps: vec!["Interview potential borrowers".into()],
        }),
        evaluations: (lease.stage == "compare").then(|| {
            (1..=count)
                .map(|candidate| CandidateEvaluation {
                    candidate,
                    scores: default_criteria()
                        .into_iter()
                        .map(|c| CriterionScore {
                            criterion: c.name,
                            score: 84 - candidate,
                            rationale: "Synthetic score illustrating a model judgment.".into(),
                        })
                        .collect(),
                    disagreements: vec!["Convenience competes with volunteer workload.".into()],
                    uncertainty: "Demand has not been independently established.".into(),
                })
                .collect()
        }),
        conversation_id: format!(
            "synthetic-{}",
            if lease.stage == "explore" {
                lease.position.to_string()
            } else {
                lease.stage.clone()
            }
        ),
        conversation_url: format!(
            "https://chatgpt.com/c/synthetic-{}",
            if lease.stage == "explore" {
                lease.position.to_string()
            } else {
                lease.stage.clone()
            }
        ),
        observed_model: "Synthetic model".into(),
        complete: true,
        error_code: None,
    }
}

use super::{client::Client, error::failure, exports, options::BrainstormOptions};
use anyhow::Result;
use jailgun_workflow::model::{
    default_criteria, ConceptRequest, Criterion, FinalResult, Run, RunAccepted,
};
use serde_json::{json, Value};
use std::{io::Read, time::Duration};

pub fn safe_text(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect()
}
pub fn run_path(id: &str) -> Result<String> {
    jailgun_core::validate_run_id(id).map_err(|_| {
        failure(
            "invalid-run-id",
            "The run ID is invalid.",
            "Use an ID returned by runs list.",
        )
    })?;
    Ok(format!("/api/runs/{id}"))
}
pub fn request(options: &BrainstormOptions) -> Result<ConceptRequest> {
    let concept = if let Some(path) = &options.concept_file {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(256_001)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 256_000 {
            return Err(failure(
                "concept-too-large",
                "The concept file exceeds 256000 UTF-8 bytes.",
                "Use at most 64000 concept characters.",
            ));
        }
        String::from_utf8(bytes).map_err(|_| {
            failure(
                "concept-invalid-text",
                "The concept file is not UTF-8.",
                "Save the concept as UTF-8 text.",
            )
        })?
    } else {
        options.concept.clone().ok_or_else(|| {
            failure(
                "concept-required",
                "A concept is required.",
                "Supply text or --concept-file.",
            )
        })?
    };
    let criteria = if options.criteria.is_empty() {
        default_criteria()
    } else {
        options
            .criteria
            .iter()
            .map(|raw| {
                let (name, weight) = raw.rsplit_once('=').ok_or_else(|| {
                    failure(
                        "criterion-invalid",
                        "Expected NAME=WEIGHT.",
                        "Repeat --criterion and use positive weights totaling 100.",
                    )
                })?;
                Ok(Criterion {
                    name: name.trim().into(),
                    weight: weight.parse().map_err(|_| {
                        failure(
                            "criterion-invalid",
                            "Weights must be positive integers.",
                            "Use criteria weights totaling 100.",
                        )
                    })?,
                })
            })
            .collect::<Result<Vec<_>>>()?
    };
    let request = ConceptRequest {
        concept,
        account_id: options.account.clone(),
        candidate_count: options.tabs,
        constraints: options.constraints.clone(),
        criteria,
        idempotency_key: options
            .idempotency_key
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
    };
    request.validate()?;
    Ok(request)
}
pub async fn brainstorm(options: BrainstormOptions, json_output: bool) -> Result<()> {
    let request = request(&options)?;
    let client = Client::connect(&options.connection).await?;
    let accepted: RunAccepted =
        client
            .post("/api/concept-runs", &request)
            .await
            .map_err(|error| {
                let code = super::error::response(&error).code;
                if !matches!(
                    code.as_str(),
                    "daemon-unavailable"
                        | "response-interrupted"
                        | "response-invalid"
                        | "response-too-large"
                ) {
                    return error;
                }
                failure(
                    "submission-unconfirmed",
                    format!("The submission response was not confirmed: {error}"),
                    format!(
                    "Inspect runs list. Retry only identical content with --idempotency-key {}.",
                    request.idempotency_key
                ),
                )
            })?;
    if !options.wait {
        if json_output {
            println!("{}", serde_json::to_string(&accepted)?);
        } else {
            println!("{}", accepted.run_id);
        }
        return Ok(());
    }
    if !json_output {
        eprintln!(
            "Run {} accepted. Waiting; interrupting this client leaves the run active.",
            accepted.run_id
        );
    }
    loop {
        let run: Run = client.get(&run_path(&accepted.run_id)?).await?;
        if run.status == "completed" {
            let result: FinalResult = client.get(&format!("/api/runs/{}/result", run.id)).await?;
            print_result(&client, &result, json_output).await?;
            return Ok(());
        }
        if !["queued", "running"].contains(&run.status.as_str()) {
            return Err(failure(
                "run-not-complete",
                format!(
                    "Run {} is {} ({}).",
                    run.id,
                    run.status,
                    run.pause_reason
                        .as_deref()
                        .unwrap_or("operator action required")
                ),
                format!(
                    "Use jailgun runs show {} and the indicated recovery action.",
                    run.id
                ),
            ));
        }
        tokio::select! {
            _=tokio::time::sleep(Duration::from_secs(1))=>{},
            _=tokio::signal::ctrl_c()=>return Err(failure("wait-interrupted",format!("Stopped waiting for {}; the run remains active.",run.id),format!("Use jailgun runs show {}.",run.id))),
        }
    }
}
pub async fn print_result(client: &Client, result: &FinalResult, json_output: bool) -> Result<()> {
    if json_output {
        println!("{}", serde_json::to_string(result)?);
        return Ok(());
    }
    let artifact = result
        .artifacts
        .iter()
        .find(|a| a.name == "final.md" && a.completion == "complete")
        .ok_or_else(|| {
            failure(
                "result-invalid",
                "The completed result has no final artifact.",
                "Inspect the stored result manifest.",
            )
        })?;
    let bytes = exports::verified(client, &result.run_id, artifact).await?;
    let text = String::from_utf8(bytes).map_err(|_| {
        failure(
            "artifact-invalid-text",
            "The final concept is not UTF-8.",
            "Inspect the artifact metadata.",
        )
    })?;
    println!("{}", safe_text(&text));
    Ok(())
}
pub fn print_value(value: &Value, json_output: bool) -> Result<()> {
    if json_output {
        println!("{}", serde_json::to_string(value)?);
    } else {
        println!("{}", safe_text(&serde_json::to_string_pretty(value)?));
    }
    Ok(())
}
pub fn empty() -> Value {
    json!({})
}

use crate::{
    database::{atomic_new, event, hash, invalid_transition, not_found},
    model::*,
    prompts,
    runs::read_run,
    scheduler::verify_lease,
    Error, Result, Store,
};
use rusqlite::{params, Connection, Transaction};
use std::{fs, path::Path};

impl Store {
    /// The adapter calls this only when generation is complete or has been stopped.
    /// Partial text is retained with the original error and never advances the pipeline.
    pub async fn capture(&self, lease: Lease, capture: Capture, now: i64) -> Result<Run> {
        self.store_capture(lease, capture, now, true).await
    }

    /// Preserve available text when stopping or ownership reconciliation cannot be confirmed.
    /// The attempt and its capacity remain reserved; this never marks a candidate complete.
    pub async fn preserve_partial(
        &self,
        lease: Lease,
        mut capture: Capture,
        now: i64,
    ) -> Result<Run> {
        capture.complete = false;
        self.store_capture(lease, capture, now, false).await
    }

    async fn store_capture(
        &self,
        lease: Lease,
        mut capture: Capture,
        now: i64,
        release: bool,
    ) -> Result<Run> {
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            verify_lease(&tx,&lease)?;
            let run = read_run(&tx,&lease.run_id)?;
            let accepted_ms:Option<i64>=tx.query_row("SELECT accepted_ms FROM attempts WHERE id=?1",[&lease.attempt_id],|r|r.get(0))?;
            if capture.error_code.is_none() {
                let timed_out=accepted_ms.is_some_and(|accepted|now.saturating_sub(accepted)>=run.configuration.response_timeout_ms);
                if now>=run.deadline_ms || timed_out {
                    capture.error_code=Some(if now>=run.deadline_ms {"run-deadline"} else {"response-timeout"}.into());
                    capture.complete=false;
                }
            }
            let (state, conversation, model, url): (String,Option<String>,Option<String>,Option<String>) = tx.query_row("SELECT state,conversation_id,observed_model,conversation_url FROM attempts WHERE id=?1", [&lease.attempt_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
            if !["accepted","uncertain"].contains(&state.as_str()) { return Err(invalid_transition()); }
            if conversation.as_ref().is_none_or(|s| s!=&capture.conversation_id) || model.as_ref().is_none_or(|s| s!=&capture.observed_model) || url.as_ref().is_none_or(|s| s!=&capture.conversation_url) {
                return Err(Error::action("capture-ownership-mismatch", "The response belongs to a different conversation or model.", "Reconcile the owned browser conversation."));
            }
            let validation = validate_capture(&run,&lease,&capture);
            let error_code = capture.error_code.clone().or_else(|| validation.as_ref().err().map(|e| e.code().to_owned()));
            let complete = capture.complete && error_code.is_none();
            let mut written = Vec::new();
            if !capture.markdown.trim().is_empty() {
                let name = if lease.stage=="explore" { format!("candidate-{}.md",lease.position) } else if lease.stage=="revise" { "final.md".into() } else { format!("{}.md",lease.stage) };
                written.push(write(&tx,&db.artifacts,&lease.run_id,Some(&lease.attempt_id),&name,"text/markdown; charset=utf-8",capture.markdown.as_bytes(),complete,now)?);
            }
            if complete {
                if let Some(summary)=&capture.summary {
                    let bytes = serde_json::to_vec_pretty(summary)?;
                    written.push(write(&tx,&db.artifacts,&lease.run_id,Some(&lease.attempt_id),&format!("candidate-{}.summary.json",lease.position),"application/json",&bytes,true,now)?);
                    tx.execute("UPDATE tasks SET summary_json=?2 WHERE id=?1", params![lease.task_id,serde_json::to_string(summary)?])?;
                }
                if let Ok(Some(ranking))=validation {
                    written.push(write(&tx,&db.artifacts,&lease.run_id,Some(&lease.attempt_id),"comparison.json","application/json",&serde_json::to_vec_pretty(&ranking)?,true,now)?);
                }
            }
            let attempt_state = if !release {"uncertain"} else if complete { "completed" } else if written.is_empty() { "failed" } else { "partial" };
            let model_mismatch=error_code.as_deref()==Some("model-mismatch");
            let retryable = capture.complete && capture.error_code.is_none() && !complete && !model_mismatch;
            let task = run.tasks.iter().find(|t| t.id==lease.task_id).ok_or_else(not_found)?;
            let task_state = if !release {"running"} else if retryable && task.attempts<2 && run.status!="cancelled" { "queued" } else { attempt_state };
            tx.execute("UPDATE attempts SET state=?2,completed_ms=?3,error_code=?4,retryable=?5 WHERE id=?1", params![lease.attempt_id,attempt_state,now,error_code,retryable])?;
            tx.execute("UPDATE tasks SET status=?2,error_code=?3 WHERE id=?1", params![lease.task_id,task_state,error_code])?;
            if release {
                tx.execute("DELETE FROM reservations WHERE id=?1 AND attempt_id=?2", params![lease.reservation_id,lease.attempt_id])?;
                crate::recovery::settle_reconciliation(&tx,&lease.run_id,now)?;
            } else if !["completed","cancelled","failed"].contains(&run.status.as_str()) {
                tx.execute("UPDATE runs SET recovery_status=CASE WHEN status!='reconciliation-required' THEN status ELSE recovery_status END,status='reconciliation-required',pause_reason=?2 WHERE id=?1",params![lease.run_id,error_code])?;
            }
            if release && model_mismatch && !["completed","cancelled","failed"].contains(&run.status.as_str()) {
                tx.execute("UPDATE runs SET status='paused',pause_reason='model-mismatch' WHERE id=?1",[&lease.run_id])?;
            }
            if complete { tx.execute("UPDATE accounts SET rate_limit_count=0 WHERE id=?1 AND readiness='ready'", [&lease.account_id])?; }
            event(&tx,&lease.run_id,"response-captured",serde_json::json!({"attempt_id":lease.attempt_id,"status":attempt_state,"error_code":error_code,"artifacts":written}),now)?;
            if complete && lease.stage=="revise" && run.status!="cancelled" {
                let current = read_run(&tx,&lease.run_id)?;
                let manifest = FinalResult {final_artifact:written.iter().find(|a| a.name=="final.md" && a.completion=="complete").cloned(),schema_version:1,run_id:run.id,status:"completed".into(),request:run.request,configuration:run.configuration,
                    excluded_candidates:current.tasks.iter().filter(|t| t.status=="excluded").map(|t| t.position).collect(),
                    evaluation_notice:"Scores are model-generated judgments, not independently established facts.".into(),artifacts:current.artifacts};
                let manifest_artifact = write(&tx,&db.artifacts,&lease.run_id,None,"result.json","application/json",&serde_json::to_vec_pretty(&manifest)?,true,now)?;
                tx.execute("UPDATE runs SET status='completed',pause_reason=NULL WHERE id=?1", [&lease.run_id])?;
                event(&tx,&lease.run_id,"run-completed",serde_json::to_value(manifest_artifact)?,now)?;
            }
            tx.commit()?;
            read_run(&db.connection,&lease.run_id)
        }).await
    }

    pub async fn artifact(
        &self,
        run_id: String,
        artifact_id: String,
    ) -> Result<(Artifact, Vec<u8>)> {
        self.call(move |db| {
            let artifact = list(&db.connection, &run_id)?
                .into_iter()
                .find(|a| a.id == artifact_id)
                .ok_or_else(not_found)?;
            let bytes = read(&db.artifacts, &artifact)?;
            Ok((artifact, bytes))
        })
        .await
    }

    pub async fn result(&self, run_id: String) -> Result<FinalResult> {
        self.call(move |db| {
            let run = read_run(&db.connection, &run_id)?;
            if run.status != "completed" {
                return Err(Error::action(
                    "result-not-complete",
                    "This run has no completed final result.",
                    "Inspect its progress and available recovery actions.",
                ));
            }
            let artifact = run
                .artifacts
                .iter()
                .find(|a| a.name == "result.json")
                .ok_or_else(not_found)?;
            let mut result: FinalResult = serde_json::from_slice(&read(&db.artifacts, artifact)?)?;
            // Schema-1 manifests predate the explicit reference. Bind it to the
            // successful revision attempt, never a failed/partial final.md.
            let accepted: String = db.connection.query_row("SELECT a.id FROM attempts a JOIN tasks t ON t.id=a.task_id WHERE a.run_id=?1 AND t.stage='revise' AND a.state='completed' ORDER BY a.number DESC LIMIT 1", [&run_id], |r| r.get(0))?;
            result.final_artifact = run.artifacts.into_iter().find(|a| a.name=="final.md" && a.completion=="complete" && a.attempt_id.as_ref()==Some(&accepted));
            if result.final_artifact.is_none() { return Err(not_found()); }
            Ok(result)
        })
        .await
    }

    pub async fn prompt(&self, lease: Lease) -> Result<String> {
        self.call(move |db| {
            let run=read_run(&db.connection,&lease.run_id)?;
            let task=run.tasks.iter().find(|t| t.id==lease.task_id).ok_or_else(not_found)?;
            let mut data=serde_json::Map::new();
            if task.stage!="explore" {
                data.insert("candidates".into(),serde_json::json!(run.tasks.iter().filter(|t| t.stage=="explore" && t.status=="completed")
                    .map(|t| serde_json::json!({"candidate":t.position,"summary":t.summary})).collect::<Vec<_>>()));
                for stage in ["compare","synthesize","critique"] {
                    if task.stage==stage { break; }
                    let Some(prior)=run.tasks.iter().find(|t| t.stage==stage && t.status=="completed") else { continue; };
                    let name=if stage=="compare" { "comparison.json".into() } else { format!("{stage}.md") };
                    let prior_attempt: String=db.connection.query_row("SELECT id FROM attempts WHERE task_id=?1 AND state='completed' ORDER BY number DESC LIMIT 1",[&prior.id],|r|r.get(0))?;
                    let artifact=run.artifacts.iter().find(|a| a.name==name && a.attempt_id.as_ref()==Some(&prior_attempt)).ok_or_else(not_found)?;
                    let bytes=read(&db.artifacts,artifact)?;
                    let text=String::from_utf8(bytes).map_err(|_| Error::action("artifact-invalid-text","A stage artifact is not valid UTF-8.","Restore the verified artifact."))?;
                    data.insert(stage.into(),serde_json::Value::String(text));
                }
            }
            prompts::stage_prompt(&run,task,serde_json::Value::Object(data))
        }).await
    }
}

fn validate_capture(run: &Run, lease: &Lease, capture: &Capture) -> Result<Option<Comparison>> {
    if !capture.complete {
        return Err(Error::action(
            "generation-interrupted",
            "The response did not complete.",
            "Inspect the preserved partial text and owned conversation.",
        ));
    }
    if capture.markdown.trim().is_empty() {
        return Err(Error::action(
            "empty-response",
            "The completed response is empty.",
            "Retry the stage.",
        ));
    }
    if let ModelSelection::Specific { name } = &run.configuration.model {
        if &capture.observed_model != name {
            return Err(Error::action(
                "model-mismatch",
                "The observed model differs from the requested model.",
                "Select an available model or reconnect the account.",
            ));
        }
    }
    if lease.stage == "explore" {
        capture
            .summary
            .as_ref()
            .ok_or_else(|| {
                Error::action(
                    "invalid-summary",
                    "The response lacks a structured candidate summary.",
                    "Retry the candidate.",
                )
            })?
            .validate()?;
    }
    if lease.stage == "compare" {
        let evaluations = capture.evaluations.as_ref().ok_or_else(|| {
            Error::action(
                "invalid-evaluation",
                "The comparison lacks structured scores.",
                "Retry comparison.",
            )
        })?;
        let candidates = run
            .tasks
            .iter()
            .filter(|t| t.stage == "explore" && t.status == "completed")
            .map(|t| t.position)
            .collect::<Vec<_>>();
        return Ok(Some(prompts::rank(
            evaluations,
            &run.request.criteria,
            &candidates,
        )?));
    }
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
fn write(
    tx: &Transaction<'_>,
    root: &Path,
    run: &str,
    attempt: Option<&str>,
    name: &str,
    media_type: &str,
    bytes: &[u8],
    complete: bool,
    now: i64,
) -> Result<Artifact> {
    if bytes.is_empty() {
        return Err(Error::action(
            "empty-artifact",
            "Cannot store an empty response artifact.",
            "Inspect the adapter response.",
        ));
    }
    let artifact = Artifact {
        id: uuid::Uuid::new_v4().to_string(),
        run_id: run.into(),
        attempt_id: attempt.map(str::to_owned),
        name: name.into(),
        media_type: media_type.into(),
        sha256: hash(bytes),
        byte_length: bytes.len() as u64,
        completion: if complete { "complete" } else { "partial" }.into(),
        created_ms: now,
    };
    atomic_new(&root.join(&artifact.id), bytes)?;
    read(root, &artifact)?;
    tx.execute(
        "INSERT INTO artifacts VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            artifact.id,
            run,
            attempt,
            name,
            media_type,
            artifact.sha256,
            artifact.byte_length as i64,
            artifact.completion,
            now
        ],
    )?;
    Ok(artifact)
}

pub(crate) fn read(root: &Path, artifact: &Artifact) -> Result<Vec<u8>> {
    uuid::Uuid::parse_str(&artifact.id).map_err(|_| {
        Error::action(
            "artifact-integrity",
            "Invalid registered artifact ID.",
            "Inspect database integrity.",
        )
    })?;
    let path = root.join(&artifact.id);
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_file()
        || metadata.len() != artifact.byte_length
        || path.canonicalize()?.parent() != Some(root)
    {
        return Err(Error::action(
            "artifact-integrity",
            "A registered artifact has changed or escaped its root.",
            "Restore the verified artifact from backup.",
        ));
    }
    let bytes = fs::read(&path)?;
    if hash(&bytes) != artifact.sha256 {
        return Err(Error::action(
            "artifact-integrity",
            "A registered artifact failed its integrity check.",
            "Restore the verified artifact from backup.",
        ));
    }
    Ok(bytes)
}

pub(crate) fn list(connection: &Connection, run: &str) -> Result<Vec<Artifact>> {
    let mut stmt=connection.prepare("SELECT id,run_id,attempt_id,name,media_type,sha256,byte_length,completion,created_ms FROM artifacts WHERE run_id=?1 ORDER BY created_ms,id")?;
    let rows = stmt.query_map([run], |r| {
        Ok(Artifact {
            id: r.get(0)?,
            run_id: r.get(1)?,
            attempt_id: r.get(2)?,
            name: r.get(3)?,
            media_type: r.get(4)?,
            sha256: r.get(5)?,
            byte_length: r.get::<_, i64>(6)? as u64,
            completion: r.get(7)?,
            created_ms: r.get(8)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

use crate::{
    database::{event, hash, invalid_transition, not_found},
    model::*,
    Error, Result, Store,
};
use rusqlite::{params, Connection, OptionalExtension};

impl Store {
    pub async fn submit(
        &self,
        request: ConceptRequest,
        model: ModelSelection,
        now: i64,
    ) -> Result<Run> {
        self.submit_with_model(request, Some(model), now).await
    }

    /// Resolve mutable account settings only for a new request, on the writer queue.
    /// Callers must authorize the requested account before invoking this service.
    pub async fn submit_for_account(&self, request: ConceptRequest, now: i64) -> Result<Run> {
        self.submit_with_model(request, None, now).await
    }

    async fn submit_with_model(
        &self,
        request: ConceptRequest,
        model: Option<ModelSelection>,
        now: i64,
    ) -> Result<Run> {
        request.validate()?;
        self.call(move |db| {
            let request_json = serde_json::to_string(&request)?;
            let content_hash = hash(request_json.as_bytes());
            let tx = db.connection.transaction()?;
            let previous: Option<(String,String)> = tx.query_row("SELECT r.id,i.content_sha256 FROM runs r JOIN request_identities i ON i.run_id=r.id WHERE r.idempotency_key=?1", [&request.idempotency_key], |r| Ok((r.get(0)?,r.get(1)?))).optional()?;
            if let Some((id, prior_hash)) = previous {
                if prior_hash != content_hash { return Err(Error::action("idempotency-conflict", "This idempotency key was used for different content.", "Reuse the original request or choose a new key.")); }
                tx.commit()?;
                return read_run(&db.connection, &id);
            }
            let model = match model {
                Some(model) => model,
                None => crate::account_sessions::read_account_model(&tx, &request.account_id)?,
            };
            if let ModelSelection::Specific { name } = &model {
                if name.trim().is_empty() || name.len() > 200 {
                    return Err(Error::action("invalid-model", "Choose an observed model or the account's current selection.", "Check the account model list."));
                }
            }
            // Retain the original snapshot hash for compatibility and audit history.
            let request_hash = hash(serde_json::to_string(&(&request, &model))?.as_bytes());
            let snapshot = ConfigurationSnapshot::new(request.candidate_count, model);
            let readiness: Option<String> = tx.query_row("SELECT readiness FROM accounts WHERE id=?1", [&request.account_id], |r| r.get(0)).optional()?;
            let readiness = readiness.ok_or_else(|| Error::action("account-not-found", "The requested account is not registered.", "Connect an account in the operator dashboard."))?;
            let id = uuid::Uuid::new_v4().to_string();
            let status = if readiness=="ready" || readiness=="cooldown" { "queued" } else { "waiting-for-auth" };
            tx.execute("INSERT INTO runs(id,account_id,request_json,snapshot_json,request_hash,idempotency_key,status,created_ms,deadline_ms,submission_limit) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![id, request.account_id, request_json, serde_json::to_string(&snapshot)?, request_hash, request.idempotency_key, status, now, now.saturating_add(RUN_DEADLINE_MS), 2*(request.candidate_count+4)])?;
            tx.execute("INSERT INTO request_identities(run_id,content_sha256) VALUES(?1,?2)", params![id,content_hash])?;
            for position in 1..=request.candidate_count { insert_task(&tx,&id,"explore",position)?; }
            for stage in ["compare","synthesize","critique","revise"] { insert_task(&tx,&id,stage,0)?; }
            event(&tx,&id,"run-created",serde_json::json!({"candidate_count":request.candidate_count,"status":status}),now)?;
            tx.commit()?;
            read_run(&db.connection,&id)
        }).await
    }

    pub async fn run(&self, id: String) -> Result<Run> {
        self.call(move |db| read_run(&db.connection, &id)).await
    }

    pub async fn runs(&self) -> Result<Vec<Run>> {
        self.call(|db| {
            let mut statement = db
                .connection
                .prepare("SELECT id FROM runs ORDER BY created_ms DESC,id LIMIT 1000")?;
            let ids = statement
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            ids.iter().map(|id| read_run(&db.connection, id)).collect()
        })
        .await
    }

    pub async fn events(&self, id: String, after: i64) -> Result<Vec<Event>> {
        self.call(move |db| {
            read_run(&db.connection,&id)?;
            let mut stmt = db.connection.prepare("SELECT sequence,kind,data_json,created_ms FROM events WHERE run_id=?1 AND sequence>?2 ORDER BY sequence LIMIT 1000")?;
            let rows = stmt.query_map(params![id,after], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?)))?;
            rows.map(|r| { let (sequence,kind,json,created_ms) = r?; Ok(Event { sequence, run_id:id.clone(), kind, data:serde_json::from_str(&json)?, created_ms }) }).collect()
        }).await
    }

    pub async fn pause(&self, id: String, now: i64) -> Result<Run> {
        self.suspend(id, "operator-paused".into(), now).await
    }

    pub async fn suspend(&self, id: String, reason: String, now: i64) -> Result<Run> {
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            let run = read_run(&tx, &id)?;
            if run.status == "paused" {
                return Ok(run);
            }
            if !["queued", "running", "waiting-for-auth"].contains(&run.status.as_str()) {
                return Err(invalid_transition());
            }
            tx.execute(
                "UPDATE runs SET status='paused',pause_reason=?2 WHERE id=?1",
                params![id, reason],
            )?;
            event(
                &tx,
                &id,
                "run-paused",
                serde_json::json!({"reason":reason}),
                now,
            )?;
            tx.commit()?;
            read_run(&db.connection, &id)
        })
        .await
    }

    pub async fn resume(&self, id: String, allow_incomplete: bool, now: i64) -> Result<Run> {
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            let run = read_run(&tx,&id)?;
            if now>=run.deadline_ms || run.submissions>=run.submission_limit {
                return Err(Error::action(if now>=run.deadline_ms { "run-deadline" } else { "submission-budget-exhausted" }, "This run has exhausted its immutable deadline or submission budget.", "Retain or export its results, then create a new run from the original inputs."));
            }
            if ["running","queued"].contains(&run.status.as_str()) && !allow_incomplete { return Ok(run); }
            if run.status!="paused" || now>=run.deadline_ms { return Err(invalid_transition()); }
            if allow_incomplete {
                let active: i64 = tx.query_row("SELECT count(*) FROM reservations WHERE run_id=?1", [&id], |r| r.get(0))?;
                let complete = run.tasks.iter().filter(|t| t.stage=="explore" && t.status=="completed").count();
                let beyond_explore = run.tasks.iter().any(|t| t.stage!="explore" && t.status!="queued");
                if complete<3 || active>0 || beyond_explore { return Err(Error::action("subset-unavailable", "Continuing requires at least three complete candidates, no active attempts, and comparison not yet started.", "Wait for active attempts or retry failed candidates.")); }
                tx.execute("UPDATE tasks SET status='excluded' WHERE run_id=?1 AND stage='explore' AND status!='completed'", [&id])?;
                tx.execute("UPDATE runs SET allow_incomplete=1 WHERE id=?1", [&id])?;
            } else {
                // This explicit operator action retries stopped failed/partial attempts, within the unchanged run budget.
                // Uncertain submissions remain running under reconciliation and cannot enter this path.
                tx.execute("UPDATE tasks SET status='queued',error_code=NULL WHERE run_id=?1 AND status IN ('failed','partial') AND attempts<28 AND EXISTS(SELECT 1 FROM attempts a WHERE a.task_id=tasks.id AND a.number=tasks.attempts AND a.state IN ('failed','partial'))", [&id])?;
            }
            let account: String = tx.query_row("SELECT readiness FROM accounts WHERE id=?1", [&run.request.account_id], |r| r.get(0))?;
            let status = if account=="ready" || account=="cooldown" { "queued" } else { "waiting-for-auth" };
            tx.execute("UPDATE runs SET status=?2,pause_reason=NULL WHERE id=?1", params![id,status])?;
            event(&tx,&id,"run-resumed",serde_json::json!({"allow_incomplete":allow_incomplete}),now)?;
            tx.commit()?;
            read_run(&db.connection,&id)
        }).await
    }

    /// Cancellation is durable. Reservations remain until the supervisor acknowledges owned pages are stopped.
    pub async fn cancel(&self, id: String, now: i64) -> Result<Run> {
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            let run = read_run(&tx, &id)?;
            if run.status == "cancelled" {
                return Ok(run);
            }
            if ["completed", "failed"].contains(&run.status.as_str()) {
                return Err(invalid_transition());
            }
            tx.execute(
                "UPDATE runs SET status='cancelled',pause_reason=NULL WHERE id=?1",
                [&id],
            )?;
            event(&tx, &id, "run-cancelled", serde_json::json!({}), now)?;
            tx.commit()?;
            read_run(&db.connection, &id)
        })
        .await
    }
}

fn insert_task(
    tx: &rusqlite::Transaction<'_>,
    run: &str,
    stage: &str,
    position: u16,
) -> Result<()> {
    tx.execute(
        "INSERT INTO tasks(id,run_id,stage,position,status) VALUES(?1,?2,?3,?4,'queued')",
        params![uuid::Uuid::new_v4().to_string(), run, stage, position],
    )?;
    Ok(())
}

pub(crate) fn read_run(connection: &Connection, id: &str) -> Result<Run> {
    let base = connection.query_row("SELECT request_json,snapshot_json,status,pause_reason,created_ms,deadline_ms,submissions,submission_limit,allow_incomplete FROM runs WHERE id=?1", [id], |r| {
        Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<String>>(3)?,r.get::<_,i64>(4)?,r.get::<_,i64>(5)?,r.get::<_,u16>(6)?,r.get::<_,u16>(7)?,r.get::<_,bool>(8)?))
    }).optional()?.ok_or_else(not_found)?;
    let mut stmt = connection.prepare("SELECT id,stage,position,status,attempts,summary_json,error_code FROM tasks WHERE run_id=?1 ORDER BY CASE stage WHEN 'explore' THEN 0 WHEN 'compare' THEN 1 WHEN 'synthesize' THEN 2 WHEN 'critique' THEN 3 ELSE 4 END,position")?;
    let rows = stmt.query_map([id], |r| {
        Ok((
            Task {
                id: r.get(0)?,
                stage: r.get(1)?,
                position: r.get(2)?,
                status: r.get(3)?,
                attempts: r.get(4)?,
                summary: None,
                error_code: r.get(6)?,
            },
            r.get::<_, Option<String>>(5)?,
        ))
    })?;
    let tasks = rows
        .map(|r| {
            let (mut task, summary) = r?;
            task.summary = summary.map(|s| serde_json::from_str(&s)).transpose()?;
            Ok(task)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Run {
        id: id.into(),
        request: serde_json::from_str(&base.0)?,
        configuration: serde_json::from_str(&base.1)?,
        status: base.2,
        pause_reason: base.3,
        created_ms: base.4,
        deadline_ms: base.5,
        submissions: base.6,
        submission_limit: base.7,
        allow_incomplete: base.8,
        tasks,
        artifacts: crate::artifacts::list(connection, id)?,
    })
}

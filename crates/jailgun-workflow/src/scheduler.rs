use crate::{
    database::{event, invalid_transition},
    model::*,
    runs::read_run,
    Error, Result, Store,
};
use rusqlite::{params, OptionalExtension};

#[derive(Debug, Clone, Copy)]
pub enum FailureDisposition {
    DefinitelyNotAccepted,
    CompletedRetryable,
    Uncertain,
    Terminal,
}

impl Store {
    /// Reserves one conversation transactionally across every run on the account.
    pub async fn claim(&self, now: i64, jitter_ms: i64) -> Result<Option<Lease>> {
        if !(0..=MAX_JITTER_MS).contains(&jitter_ms) {
            return Err(Error::action(
                "invalid-jitter",
                "Submission jitter must be between zero and five seconds.",
                "Use the stored pacing configuration.",
            ));
        }
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            // Deadlines also apply while paused or waiting for authentication.
            let expired = {
                let mut stmt = tx.prepare("SELECT id FROM runs WHERE status IN ('queued','running','paused','waiting-for-auth') AND deadline_ms<=?1 AND coalesce(pause_reason,'')!='run-deadline'")?;
                let ids = stmt.query_map([now], |r| r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
                ids
            };
            for id in expired { pause(&tx, &id, "run-deadline", now)?; }
            let ids = {
                let mut stmt = tx.prepare("SELECT r.id FROM runs r JOIN accounts a ON a.id=r.account_id WHERE r.status IN ('queued','running') ORDER BY coalesce((SELECT max(e.sequence) FROM events e WHERE e.run_id=r.id AND e.kind='attempt-prepared'),0),r.created_ms,r.id")?;
                let ids=stmt.query_map([], |r| r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
                ids
            };
            for id in ids {
                let run = read_run(&tx,&id)?;
                if now>=run.deadline_ms || run.submissions>=run.submission_limit {
                    let reason = if now>=run.deadline_ms { "run-deadline" } else { "submission-budget-exhausted" };
                    pause(&tx,&id,reason,now)?;
                    continue;
                }
                let (ready, available_at, cooldown, capacity, active): (String,i64,i64,u16,u16) = tx.query_row("SELECT readiness,next_submit_ms,cooldown_until_ms,min(capacity,10),(SELECT count(*) FROM reservations WHERE account_id=accounts.id) FROM accounts WHERE id=?1", [&run.request.account_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
                if ready!="ready" || now<available_at || now<cooldown || active>=capacity { continue; }
                let Some(task) = next_task(&run) else {
                    if run.tasks.iter().all(|t| t.status!="running") && run.tasks.iter().any(|t| ["failed","partial"].contains(&t.status.as_str())) {
                        pause(&tx,&id,"incomplete-results",now)?;
                    }
                    continue;
                };
                let lease = Lease { reservation_id:uuid::Uuid::new_v4().to_string(), account_id:run.request.account_id.clone(), run_id:id.clone(), task_id:task.id.clone(), attempt_id:uuid::Uuid::new_v4().to_string(), stage:task.stage.clone(), position:task.position };
                tx.execute("UPDATE tasks SET status='running',attempts=attempts+1,error_code=NULL WHERE id=?1 AND status='queued'", [&task.id])?;
                tx.execute("INSERT INTO attempts(id,task_id,run_id,number,state,created_ms) VALUES(?1,?2,?3,?4,'prepared',?5)", params![lease.attempt_id,task.id,id,task.attempts+1,now])?;
                tx.execute("INSERT INTO reservations VALUES(?1,?2,?3,?4,?5,?6)", params![lease.reservation_id,lease.account_id,id,lease.attempt_id,now,SUBMISSION_SPACING_MS+jitter_ms])?;
                tx.execute("UPDATE accounts SET next_submit_ms=?2 WHERE id=?1", params![lease.account_id,now.saturating_add(SUBMISSION_SPACING_MS+jitter_ms)])?;
                tx.execute("UPDATE runs SET status='running' WHERE id=?1", [&id])?;
                event(&tx,&id,"attempt-prepared",serde_json::to_value(&lease)?,now)?;
                tx.commit()?;
                return Ok(Some(lease));
            }
            tx.commit()?;
            Ok(None)
        }).await
    }

    /// Commit uncertainty and charge the budget before the provider receives any bytes.
    pub async fn begin_submission(&self, lease: Lease, now: i64) -> Result<()> {
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            verify_lease(&tx,&lease)?;
            let run = read_run(&tx,&lease.run_id)?;
            let (ready,allowed): (String,i64) = tx.query_row("SELECT readiness,submission_allowed_ms FROM accounts WHERE id=?1", [&lease.account_id], |r| Ok((r.get(0)?,r.get(1)?)))?;
            if !["running","queued"].contains(&run.status.as_str()) || ready!="ready" || now>=run.deadline_ms || run.submissions>=run.submission_limit { return Err(invalid_transition()); }
            if now<allowed { return Err(Error::action("submission-paced","Another conversation submitted recently on this account.","Keep the reservation and wait for the account submission time.")); }
            let spacing:i64=tx.query_row("SELECT spacing_ms FROM reservations WHERE id=?1",[&lease.reservation_id],|r|r.get(0))?;
            if tx.execute("UPDATE attempts SET state='submitting' WHERE id=?1 AND state='prepared'", [&lease.attempt_id])? != 1 { return Err(invalid_transition()); }
            tx.execute("UPDATE runs SET status='running',submissions=submissions+1 WHERE id=?1", [&lease.run_id])?;
            tx.execute("UPDATE accounts SET submission_allowed_ms=?2,next_submit_ms=max(next_submit_ms,?2) WHERE id=?1",params![lease.account_id,now.saturating_add(spacing)])?;
            event(&tx,&lease.run_id,"submission-started",serde_json::json!({"attempt_id":lease.attempt_id}),now)?;
            tx.commit()?;
            Ok(())
        }).await
    }

    pub async fn accepted(&self, lease: Lease, turn: AcceptedTurn, now: i64) -> Result<()> {
        let url = url::Url::parse(&turn.conversation_url).map_err(|_| {
            Error::action(
                "adapter-invalid-acceptance",
                "Invalid conversation URL.",
                "Reconcile the owned conversation.",
            )
        })?;
        let production = url.scheme() == "https" && url.host_str() == Some("chatgpt.com");
        let fixture =
            url.scheme() == "http" && matches!(url.host_str(), Some("127.0.0.1" | "localhost"));
        if turn.conversation_id.trim().is_empty()
            || turn.user_turn_id.trim().is_empty()
            || turn.observed_model.trim().is_empty()
            || (!production && !fixture)
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != format!("/c/{}", turn.conversation_id)
        {
            return Err(Error::action(
                "adapter-invalid-acceptance",
                "The provider did not identify the accepted conversation, turn and model.",
                "Reconcile the conversation before retrying.",
            ));
        }
        self.call(move |db| {
            let tx=db.connection.transaction()?;
            verify_lease(&tx,&lease)?;
            if tx.execute("UPDATE attempts SET state='accepted',conversation_id=?2,conversation_url=?3,observed_model=?4,accepted_ms=?5,user_turn_id=?6 WHERE id=?1 AND state='submitting'",params![lease.attempt_id,turn.conversation_id,turn.conversation_url,turn.observed_model,now,turn.user_turn_id])?!=1 {return Err(invalid_transition());}
            event(&tx,&lease.run_id,"prompt-accepted",serde_json::json!({"attempt_id":lease.attempt_id,"conversation_id":turn.conversation_id,"user_turn_id":turn.user_turn_id,"observed_model":turn.observed_model}),now)?;
            tx.commit()?;
            Ok(())
        }).await
    }

    pub async fn fail_attempt(
        &self,
        lease: Lease,
        code: String,
        disposition: FailureDisposition,
        now: i64,
    ) -> Result<()> {
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            verify_lease(&tx,&lease)?;
            let (state, number): (String,u16) = tx.query_row("SELECT state,number FROM attempts WHERE id=?1", [&lease.attempt_id], |r| Ok((r.get(0)?,r.get(1)?)))?;
            if !["prepared","submitting","accepted","uncertain"].contains(&state.as_str()) { return Err(invalid_transition()); }
            if matches!(disposition, FailureDisposition::DefinitelyNotAccepted) && state=="accepted" { return Err(invalid_transition()); }
            if matches!(disposition, FailureDisposition::CompletedRetryable) && state!="accepted" { return Err(invalid_transition()); }
            let run = read_run(&tx,&lease.run_id)?;
            let uncertain = matches!(disposition, FailureDisposition::Uncertain);
            let retryable = matches!(disposition, FailureDisposition::DefinitelyNotAccepted|FailureDisposition::CompletedRetryable);
            tx.execute("UPDATE attempts SET state=?2,error_code=?3,retryable=?4,completed_ms=?5 WHERE id=?1", params![lease.attempt_id,if uncertain {"uncertain"} else {"failed"},code,retryable,now])?;
            let task_state = if uncertain { "running" } else if retryable && number<2 && run.status!="cancelled" { "queued" } else { "failed" };
            tx.execute("UPDATE tasks SET status=?2,error_code=?3 WHERE id=?1", params![lease.task_id,task_state,code])?;
            if uncertain {
                if !["cancelled","completed","failed"].contains(&run.status.as_str()) {
                    tx.execute("UPDATE runs SET recovery_status=CASE WHEN status!='reconciliation-required' THEN status ELSE recovery_status END,status='reconciliation-required',pause_reason=?2 WHERE id=?1", params![lease.run_id,code])?;
                }
            } else {
                tx.execute("DELETE FROM reservations WHERE id=?1 AND attempt_id=?2", params![lease.reservation_id,lease.attempt_id])?;
                crate::recovery::settle_reconciliation(&tx,&lease.run_id,now)?;
            }
            event(&tx,&lease.run_id,"attempt-failed",serde_json::json!({"attempt_id":lease.attempt_id,"error_code":code,"uncertain":uncertain,"retryable":retryable}),now)?;
            tx.commit()?;
            Ok(())
        }).await
    }

    /// Call only after the supervisor confirms the affected conversation stopped.
    pub async fn acknowledge_cancel(&self, lease: Lease, now: i64) -> Result<()> {
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM reservations WHERE id=?1)", [&lease.reservation_id], |r| r.get(0))?;
            if !exists { return Ok(()); }
            verify_lease(&tx,&lease)?;
            if read_run(&tx,&lease.run_id)?.status!="cancelled" { return Err(invalid_transition()); }
            tx.execute("UPDATE attempts SET state='cancelled',completed_ms=?2 WHERE id=?1 AND state IN ('prepared','submitting','accepted','uncertain')", params![lease.attempt_id,now])?;
            tx.execute("UPDATE tasks SET status='failed',error_code='cancelled' WHERE id=?1 AND status='running'", [&lease.task_id])?;
            tx.execute("DELETE FROM reservations WHERE id=?1", [&lease.reservation_id])?;
            event(&tx,&lease.run_id,"conversation-stopped",serde_json::json!({"attempt_id":lease.attempt_id}),now)?;
            tx.commit()?;
            Ok(())
        }).await
    }
}

fn next_task(run: &Run) -> Option<&Task> {
    let explore = run
        .tasks
        .iter()
        .filter(|t| t.stage == "explore")
        .collect::<Vec<_>>();
    let exploration_done = explore
        .iter()
        .all(|t| t.status == "completed" || (run.allow_incomplete && t.status == "excluded"))
        && explore.iter().filter(|t| t.status == "completed").count() >= 3;
    for task in &run.tasks {
        if task.status != "queued" || task.attempts >= 28 {
            continue;
        }
        let prerequisite = match task.stage.as_str() {
            "explore" => return Some(task),
            "compare" => None,
            "synthesize" => Some("compare"),
            "critique" => Some("synthesize"),
            "revise" => Some("critique"),
            _ => continue,
        };
        if exploration_done
            && prerequisite.is_none_or(|stage| {
                run.tasks
                    .iter()
                    .any(|t| t.stage == stage && t.status == "completed")
            })
        {
            return Some(task);
        }
    }
    None
}

pub(crate) fn verify_lease(tx: &rusqlite::Transaction<'_>, lease: &Lease) -> Result<()> {
    let matches: Option<bool> = tx.query_row("SELECT r.account_id=?2 AND r.run_id=?3 AND r.attempt_id=?4 AND a.task_id=?5 AND t.stage=?6 AND t.position=?7 FROM reservations r JOIN attempts a ON a.id=r.attempt_id JOIN tasks t ON t.id=a.task_id WHERE r.id=?1",
        params![lease.reservation_id,lease.account_id,lease.run_id,lease.attempt_id,lease.task_id,lease.stage,lease.position], |r| r.get(0)).optional()?;
    if matches != Some(true) {
        return Err(Error::action(
            "ownership-mismatch",
            "The reservation does not own this run and attempt.",
            "Refresh ownership before changing any conversation.",
        ));
    }
    Ok(())
}

fn pause(tx: &rusqlite::Transaction<'_>, id: &str, reason: &str, now: i64) -> Result<()> {
    tx.execute(
        "UPDATE runs SET status='paused',pause_reason=?2 WHERE id=?1",
        params![id, reason],
    )?;
    event(
        tx,
        id,
        "run-paused",
        serde_json::json!({"reason":reason}),
        now,
    )?;
    Ok(())
}

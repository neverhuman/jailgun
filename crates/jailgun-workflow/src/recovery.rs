use crate::{
    artifacts,
    database::{atomic_new, event, new_private_dir, private_dir, private_file},
    model::*,
    runs::read_run,
    scheduler::verify_lease,
    Error, Result, Store,
};
use rusqlite::params;
use std::{fs, path::PathBuf};

impl Store {
    /// The supervisor supplies only attempts it has just reattached successfully.
    /// Every remaining reservation must be accepted and present; uncertainty blocks resume.
    pub async fn resume_reattached(
        &self,
        run_id: String,
        attempt_ids: Vec<String>,
        now: i64,
    ) -> Result<bool> {
        self.call(move|db| {
            let tx=db.connection.transaction()?;
            if read_run(&tx,&run_id)?.status!="reconciliation-required" {return Ok(false);}
            for lease in active_leases(&tx)?.into_iter().filter(|lease|lease.run_id==run_id) {
                let accepted:bool=tx.query_row("SELECT state='accepted' AND user_turn_id IS NOT NULL FROM attempts WHERE id=?1",[&lease.attempt_id],|r|r.get(0))?;
                if !accepted || !attempt_ids.contains(&lease.attempt_id) {return Ok(false);}
            }
            tx.execute("UPDATE runs SET status=CASE WHEN recovery_status='paused' THEN 'paused' ELSE 'queued' END,pause_reason=CASE WHEN recovery_status='paused' THEN 'operator-paused' ELSE NULL END,recovery_status=NULL WHERE id=?1",[&run_id])?;
            event(&tx,&run_id,"conversations-reconciled",serde_json::json!({"attempt_ids":attempt_ids}),now)?;
            tx.commit()?;
            Ok(true)
        }).await
    }

    /// Reconcile after acquiring the exclusive daemon ownership lock, before scheduling.
    /// Accepted or ambiguous submissions keep their reservations until browser reconciliation.
    pub async fn recover(&self, now: i64) -> Result<Vec<Lease>> {
        self.call(move |db| {
            let tx=db.connection.transaction()?;
            let leases=active_leases(&tx)?;
            let mut uncertain=Vec::new();
            for lease in leases {
                let run=read_run(&tx,&lease.run_id)?;
                let state:String=tx.query_row("SELECT state FROM attempts WHERE id=?1",[&lease.attempt_id],|r|r.get(0))?;
                if state=="prepared" {
                    // No submission was allowed before begin_submission committed.
                    tx.execute("UPDATE attempts SET state='failed',error_code='interrupted-before-submission',retryable=1,completed_ms=?2 WHERE id=?1",params![lease.attempt_id,now])?;
                    tx.execute("UPDATE tasks SET status=CASE WHEN attempts<2 THEN 'queued' ELSE 'failed' END,error_code='interrupted-before-submission' WHERE id=?1",[&lease.task_id])?;
                    tx.execute("DELETE FROM reservations WHERE id=?1",[&lease.reservation_id])?;
                    event(&tx,&lease.run_id,"attempt-recovered-before-submission",serde_json::json!({"attempt_id":lease.attempt_id}),now)?;
                } else {
                    if state=="submitting" { tx.execute("UPDATE attempts SET state='uncertain',error_code='submission-uncertain' WHERE id=?1",[&lease.attempt_id])?; }
                    if !["cancelled","completed","failed","reconciliation-required"].contains(&run.status.as_str()) {
                        tx.execute("UPDATE runs SET recovery_status=status,status='reconciliation-required',pause_reason='restart-reconciliation' WHERE id=?1",[&lease.run_id])?;
                        event(&tx,&lease.run_id,"reconciliation-required",serde_json::json!({"attempt_id":lease.attempt_id}),now)?;
                    }
                    uncertain.push(lease);
                }
            }
            tx.commit()?;
            Ok(uncertain)
        }).await
    }

    pub async fn active_leases(&self) -> Result<Vec<Lease>> {
        self.call(|db| active_leases(&db.connection)).await
    }

    pub async fn attempts(&self, run_id: String) -> Result<Vec<Attempt>> {
        self.call(move |db| {
            read_run(&db.connection,&run_id)?;
            let mut stmt=db.connection.prepare("SELECT id,run_id,task_id,number,state,conversation_id,conversation_url,observed_model,created_ms,accepted_ms,completed_ms,error_code,user_turn_id FROM attempts WHERE run_id=?1 ORDER BY created_ms,id")?;
            let rows=stmt.query_map([run_id],|r|Ok(Attempt { id:r.get(0)?,run_id:r.get(1)?,task_id:r.get(2)?,number:r.get(3)?,state:r.get(4)?,conversation_id:r.get(5)?,conversation_url:r.get(6)?,observed_model:r.get(7)?,created_ms:r.get(8)?,accepted_ms:r.get(9)?,completed_ms:r.get(10)?,error_code:r.get(11)?,user_turn_id:r.get(12)? }))?;
            Ok(rows.collect::<std::result::Result<Vec<_>,_>>()?)
        }).await
    }

    /// Only a known persisted conversation can be reattached. Never sends a prompt.
    pub async fn reattached(&self, lease: Lease, conversation_id: String, now: i64) -> Result<()> {
        self.call(move |db| {
            let tx=db.connection.transaction()?;
            verify_lease(&tx,&lease)?;
            let matches:bool=tx.query_row("SELECT state='accepted' AND conversation_id=?2 AND user_turn_id IS NOT NULL FROM attempts WHERE id=?1",params![lease.attempt_id,conversation_id],|r|r.get(0))?;
            if !matches { return Err(Error::action("reconciliation-required","The persisted acceptance record does not identify this conversation.","Inspect submission status; no automatic resubmission is allowed.")); }
            event(&tx,&lease.run_id,"conversation-reattached",serde_json::json!({"attempt_id":lease.attempt_id,"conversation_id":conversation_id}),now)?;
            tx.commit()?;
            Ok(())
        }).await
    }

    /// Consistent SQLite online backup plus every registered artifact, serialized with writes.
    /// Browser profiles stay in place and require a separate stopped-browser backup.
    pub async fn backup(&self, destination: PathBuf) -> Result<()> {
        self.call(move |db| {
            new_private_dir(&destination, "backup-exists")?;
            let destination=destination.canonicalize()?;
            private_dir(&destination.join("artifacts"))?;
            let database=destination.join("workflows.sqlite3");
            private_file(&database,true)?;
            db.connection.backup(rusqlite::MAIN_DB,&database,None)?;
            fs::File::open(&database)?.sync_all()?;
            let mut statement=db.connection.prepare("SELECT id FROM runs ORDER BY id")?;
            let runs=statement.query_map([],|r|r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
            for run in runs {
                for artifact in artifacts::list(&db.connection,&run)? {
                    atomic_new(&destination.join("artifacts").join(&artifact.id),&artifacts::read(&db.artifacts,&artifact)?)?;
                }
            }
            let manifest=serde_json::json!({"schema_version":crate::database::SCHEMA_VERSION,"sqlite_version":rusqlite::version(),"database_sha256":crate::database::hash(&fs::read(&database)?),"profiles_included":false});
            atomic_new(&destination.join("backup.json"),&serde_json::to_vec_pretty(&manifest)?)?;
            Ok(())
        }).await
    }
}

pub(crate) fn active_leases(connection: &rusqlite::Connection) -> Result<Vec<Lease>> {
    let mut stmt=connection.prepare("SELECT r.id,r.account_id,r.run_id,a.task_id,a.id,t.stage,t.position FROM reservations r JOIN attempts a ON a.id=r.attempt_id JOIN tasks t ON t.id=a.task_id ORDER BY r.created_ms,r.id")?;
    let rows = stmt.query_map([], |r| {
        Ok(Lease {
            reservation_id: r.get(0)?,
            account_id: r.get(1)?,
            run_id: r.get(2)?,
            task_id: r.get(3)?,
            attempt_id: r.get(4)?,
            stage: r.get(5)?,
            position: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub(crate) fn settle_reconciliation(
    tx: &rusqlite::Transaction<'_>,
    run: &str,
    now: i64,
) -> Result<()> {
    let count: i64 = tx.query_row(
        "SELECT count(*) FROM reservations WHERE run_id=?1",
        [run],
        |r| r.get(0),
    )?;
    if count==0 && tx.execute("UPDATE runs SET status=CASE WHEN recovery_status='paused' THEN 'paused' ELSE 'queued' END,pause_reason=CASE WHEN recovery_status='paused' THEN 'operator-paused' ELSE NULL END,recovery_status=NULL WHERE id=?1 AND status='reconciliation-required'",[run])?>0 {
        event(tx,run,"reconciliation-completed",serde_json::json!({}),now)?;
    }
    Ok(())
}

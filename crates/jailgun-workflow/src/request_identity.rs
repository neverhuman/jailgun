use crate::{
    database::hash,
    model::{ConceptRequest, ConfigurationSnapshot},
    Error, Result,
};

/// Keep historical configurations and hashes unchanged; fail atomically if they disagree.
pub(crate) fn backfill(tx: &rusqlite::Transaction<'_>) -> Result<()> {
    let mut statement =
        tx.prepare("SELECT id,request_json,snapshot_json,request_hash FROM runs")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    for row in rows {
        let (id, request, snapshot, original_hash) = row?;
        let request: ConceptRequest = serde_json::from_str(&request)?;
        let snapshot: ConfigurationSnapshot = serde_json::from_str(&snapshot)?;
        if hash(serde_json::to_string(&(&request, &snapshot.model))?.as_bytes()) != original_hash {
            return Err(Error::action(
                "request-identity-conflict",
                "A historical request disagrees with its recorded snapshot hash.",
                "Preserve the runtime and investigate before retrying migration.",
            ));
        }
        tx.execute(
            "INSERT INTO request_identities(run_id,content_sha256) VALUES(?1,?2)",
            rusqlite::params![id, hash(serde_json::to_string(&request)?.as_bytes())],
        )?;
    }
    Ok(())
}

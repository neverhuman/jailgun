use crate::{
    database::{hash, not_found},
    model::{CreatedToken, IssueToken, TokenMetadata},
    Error, Result, Store,
};
use rusqlite::{params, Connection, OptionalExtension};

impl Store {
    pub async fn issue_token(&self, request: IssueToken, now: i64) -> Result<CreatedToken> {
        if request.name.trim().is_empty()
            || request.name.chars().count() > 80
            || request.account_ids.is_empty()
            || request.account_ids.len() > 100
            || request.name.chars().any(char::is_control)
        {
            return Err(Error::action(
                "invalid-token-request",
                "A token name and one or more registered account IDs are required.",
                "Select the accounts this automation may access.",
            ));
        }
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            let id = uuid::Uuid::new_v4().to_string();
            let secret = format!(
                "jg_auto_{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            );
            tx.execute(
                "INSERT INTO automation_tokens(id,name,digest,created_ms) VALUES(?1,?2,?3,?4)",
                params![id, request.name.trim(), hash(secret.as_bytes()), now],
            )?;
            let mut accounts = request.account_ids;
            accounts.sort();
            accounts.dedup();
            for account in accounts {
                let exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=?1)",
                    [&account],
                    |r| r.get(0),
                )?;
                if !exists {
                    return Err(Error::action(
                        "invalid-token-request",
                        "An account ID is not registered.",
                        "List accounts and select registered IDs.",
                    ));
                }
                tx.execute(
                    "INSERT INTO token_accounts(token_id,account_id) VALUES(?1,?2)",
                    params![id, account],
                )?;
            }
            let metadata = read_token(&tx, &id)?;
            tx.commit()?;
            Ok(CreatedToken { metadata, secret })
        })
        .await
    }

    pub async fn tokens(&self) -> Result<Vec<TokenMetadata>> {
        self.call(|db| {
            let mut statement = db
                .connection
                .prepare("SELECT id FROM automation_tokens ORDER BY created_ms,id")?;
            let rows = statement.query_map([], |r| r.get::<_, String>(0))?;
            rows.map(|id| read_token(&db.connection, &id?)).collect()
        })
        .await
    }

    pub async fn revoke_token(&self, id: String, now: i64) -> Result<TokenMetadata> {
        self.call(move |db| {
            if db.connection.execute(
                "UPDATE automation_tokens SET revoked_ms=coalesce(revoked_ms,?2) WHERE id=?1",
                params![id, now],
            )? == 0
            {
                return Err(not_found());
            }
            read_token(&db.connection, &id)
        })
        .await
    }

    pub async fn authenticate_token(&self, secret: String) -> Result<Option<TokenMetadata>> {
        if secret.len() != 72 || !secret.starts_with("jg_auto_") {
            return Ok(None);
        }
        let digest = hash(secret.as_bytes());
        self.call(move |db| {
            let id: Option<String> = db
                .connection
                .query_row(
                    "SELECT id FROM automation_tokens WHERE digest=?1 AND revoked_ms IS NULL",
                    [digest],
                    |r| r.get(0),
                )
                .optional()?;
            id.map(|id| read_token(&db.connection, &id)).transpose()
        })
        .await
    }
}

fn read_token(connection: &Connection, id: &str) -> Result<TokenMetadata> {
    let mut token = connection
        .query_row(
            "SELECT id,name,created_ms,revoked_ms FROM automation_tokens WHERE id=?1",
            [id],
            |r| {
                Ok(TokenMetadata {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    account_ids: Vec::new(),
                    created_ms: r.get(2)?,
                    revoked_ms: r.get(3)?,
                })
            },
        )
        .optional()?
        .ok_or_else(not_found)?;
    let mut statement = connection
        .prepare("SELECT account_id FROM token_accounts WHERE token_id=?1 ORDER BY account_id")?;
    token.account_ids = statement
        .query_map([id], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(token)
}

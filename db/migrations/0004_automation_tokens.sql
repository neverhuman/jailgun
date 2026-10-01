-- SQLite busy_timeout bounds lock waits; the Rust migration runner bounds VM execution.
PRAGMA busy_timeout = 5000;
CREATE TABLE automation_tokens (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 80),
    digest TEXT NOT NULL UNIQUE CHECK(length(digest)=64),
    created_ms INTEGER NOT NULL,
    revoked_ms INTEGER
) STRICT;
CREATE TABLE token_accounts (
    token_id TEXT NOT NULL REFERENCES automation_tokens(id),
    account_id TEXT NOT NULL REFERENCES accounts(id),
    PRIMARY KEY(token_id,account_id)
) STRICT;
CREATE TRIGGER token_revocation_final BEFORE UPDATE OF revoked_ms ON automation_tokens
WHEN OLD.revoked_ms IS NOT NULL AND NEW.revoked_ms IS NOT OLD.revoked_ms
BEGIN SELECT RAISE(ABORT,'revocation is permanent'); END;

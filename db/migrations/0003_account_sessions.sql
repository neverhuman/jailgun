-- SQLite lock waits are bounded by busy_timeout; the migration runner bounds VM execution.
PRAGMA busy_timeout = 5000;

CREATE TABLE account_sessions (
    account_id TEXT PRIMARY KEY REFERENCES accounts(id),
    lifecycle TEXT NOT NULL DEFAULT 'disconnected' CHECK(lifecycle IN
      ('disconnected','starting','login-required','waiting-for-user','verifying','ready','expired','mismatch','failed','cancelled')),
    model_json TEXT CHECK(model_json IS NULL OR json_valid(model_json)),
    observation_json TEXT CHECK(observation_json IS NULL OR json_valid(observation_json)),
    observed_ms INTEGER,
    login_expires_ms INTEGER,
    error_code TEXT
) STRICT;
INSERT INTO account_sessions(account_id) SELECT id FROM accounts;
CREATE TRIGGER account_session_created AFTER INSERT ON accounts
BEGIN INSERT INTO account_sessions(account_id) VALUES(NEW.id); END;

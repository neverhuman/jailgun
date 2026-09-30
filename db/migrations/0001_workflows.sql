CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY,
    sha256 TEXT NOT NULL CHECK(length(sha256) = 64)
) STRICT;

CREATE TABLE accounts (
    id TEXT PRIMARY KEY,
    email_hint TEXT NOT NULL COLLATE NOCASE UNIQUE,
    provider_id TEXT UNIQUE,
    profile_dir TEXT NOT NULL UNIQUE,
    metadata_json TEXT NOT NULL CHECK(json_valid(metadata_json)),
    cdp_port INTEGER NOT NULL UNIQUE CHECK(cdp_port BETWEEN 1 AND 65535),
    capacity INTEGER NOT NULL CHECK(capacity > 0),
    readiness TEXT NOT NULL CHECK(readiness IN ('login-required','ready','expired','mismatch','failed','cooldown','paused')),
    next_submit_ms INTEGER NOT NULL DEFAULT 0,
    submission_allowed_ms INTEGER NOT NULL DEFAULT 0,
    cooldown_until_ms INTEGER NOT NULL DEFAULT 0,
    rate_limit_count INTEGER NOT NULL DEFAULT 0 CHECK(rate_limit_count >= 0)
) STRICT;

CREATE TABLE imports (
    source_sha256 TEXT PRIMARY KEY CHECK(length(source_sha256) = 64),
    imported_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE runs (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    request_json TEXT NOT NULL CHECK(json_valid(request_json)),
    snapshot_json TEXT NOT NULL CHECK(json_valid(snapshot_json)),
    request_hash TEXT NOT NULL CHECK(length(request_hash) = 64),
    idempotency_key TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL CHECK(status IN ('queued','running','paused','waiting-for-auth','reconciliation-required','completed','cancelled','failed')),
    pause_reason TEXT,
    recovery_status TEXT CHECK(recovery_status IS NULL OR recovery_status IN ('queued','running','paused','waiting-for-auth')),
    created_ms INTEGER NOT NULL,
    deadline_ms INTEGER NOT NULL,
    submissions INTEGER NOT NULL DEFAULT 0 CHECK(submissions >= 0),
    submission_limit INTEGER NOT NULL CHECK(submission_limit BETWEEN 18 AND 28),
    allow_incomplete INTEGER NOT NULL DEFAULT 0 CHECK(allow_incomplete IN (0,1)),
    CHECK(submissions <= submission_limit),
    CHECK(deadline_ms > created_ms)
) STRICT;

CREATE TRIGGER immutable_run_configuration BEFORE UPDATE OF
    account_id, request_json, snapshot_json, request_hash, idempotency_key,
    created_ms, deadline_ms, submission_limit ON runs
BEGIN SELECT RAISE(ABORT, 'immutable run configuration'); END;

CREATE TRIGGER terminal_run_state BEFORE UPDATE OF status ON runs
WHEN OLD.status IN ('completed','cancelled','failed') AND NEW.status != OLD.status
BEGIN SELECT RAISE(ABORT, 'terminal run state'); END;

CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(id),
    stage TEXT NOT NULL CHECK(stage IN ('explore','compare','synthesize','critique','revise')),
    position INTEGER NOT NULL CHECK(position BETWEEN 0 AND 10),
    status TEXT NOT NULL CHECK(status IN ('queued','running','completed','partial','failed','excluded')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts BETWEEN 0 AND 28),
    summary_json TEXT CHECK(summary_json IS NULL OR json_valid(summary_json)),
    error_code TEXT,
    UNIQUE(run_id,stage,position),
    UNIQUE(id,run_id),
    CHECK((stage = 'explore' AND position > 0) OR (stage != 'explore' AND position = 0))
) STRICT;

CREATE TABLE attempts (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    number INTEGER NOT NULL CHECK(number BETWEEN 1 AND 28),
    state TEXT NOT NULL CHECK(state IN ('prepared','submitting','accepted','completed','partial','failed','uncertain','cancelled')),
    conversation_id TEXT,
    conversation_url TEXT,
    observed_model TEXT,
    created_ms INTEGER NOT NULL,
    accepted_ms INTEGER,
    completed_ms INTEGER,
    error_code TEXT,
    retryable INTEGER NOT NULL DEFAULT 0 CHECK(retryable IN (0,1)),
    UNIQUE(task_id,number),
    UNIQUE(id,run_id),
    FOREIGN KEY(task_id,run_id) REFERENCES tasks(id,run_id)
) STRICT;

CREATE TRIGGER checked_attempt_transition BEFORE UPDATE OF state ON attempts
WHEN OLD.state != NEW.state AND NOT (
    (OLD.state='prepared' AND NEW.state IN ('submitting','failed','cancelled')) OR
    (OLD.state='submitting' AND NEW.state IN ('accepted','failed','uncertain','cancelled')) OR
    (OLD.state='accepted' AND NEW.state IN ('completed','partial','failed','uncertain','cancelled')) OR
    (OLD.state='uncertain' AND NEW.state IN ('accepted','completed','partial','failed','cancelled'))
)
BEGIN SELECT RAISE(ABORT, 'invalid attempt transition'); END;

CREATE TABLE reservations (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    run_id TEXT NOT NULL REFERENCES runs(id),
    attempt_id TEXT NOT NULL UNIQUE,
    created_ms INTEGER NOT NULL,
    spacing_ms INTEGER NOT NULL CHECK(spacing_ms BETWEEN 30000 AND 35000),
    FOREIGN KEY(attempt_id,run_id) REFERENCES attempts(id,run_id)
) STRICT;

CREATE TRIGGER reservation_owner BEFORE INSERT ON reservations
WHEN NEW.account_id != (SELECT account_id FROM runs WHERE id = NEW.run_id)
BEGIN SELECT RAISE(ABORT, 'reservation account mismatch'); END;

CREATE TRIGGER reservation_capacity BEFORE INSERT ON reservations
WHEN (SELECT count(*) FROM reservations WHERE account_id = NEW.account_id)
    >= min(10, (SELECT capacity FROM accounts WHERE id = NEW.account_id))
BEGIN SELECT RAISE(ABORT, 'account capacity exhausted'); END;

CREATE TABLE artifacts (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(id),
    attempt_id TEXT,
    name TEXT NOT NULL,
    media_type TEXT NOT NULL,
    sha256 TEXT NOT NULL CHECK(length(sha256) = 64),
    byte_length INTEGER NOT NULL CHECK(byte_length > 0),
    completion TEXT NOT NULL CHECK(completion IN ('complete','partial')),
    created_ms INTEGER NOT NULL,
    FOREIGN KEY(attempt_id,run_id) REFERENCES attempts(id,run_id)
) STRICT;

CREATE TABLE events (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL REFERENCES runs(id),
    kind TEXT NOT NULL,
    data_json TEXT NOT NULL CHECK(json_valid(data_json)),
    created_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX events_run_sequence ON events(run_id,sequence);
CREATE INDEX tasks_run_status ON tasks(run_id,status);
CREATE INDEX reservations_account ON reservations(account_id);

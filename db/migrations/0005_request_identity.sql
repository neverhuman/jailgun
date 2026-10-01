-- Request-only identity v1: serde JSON with defaults materialized, preserving
-- content whitespace, Unicode, criterion order and weights. Account settings
-- are excluded. Original request_hash and snapshot_json remain immutable.
CREATE TABLE request_identities (
    run_id TEXT PRIMARY KEY REFERENCES runs(id),
    content_sha256 TEXT NOT NULL CHECK(length(content_sha256) = 64)
) STRICT;
CREATE TRIGGER immutable_request_identity BEFORE UPDATE ON request_identities
BEGIN SELECT RAISE(ABORT, 'immutable request identity'); END;
-- Rust backfills and verifies historical snapshot hashes in this transaction.

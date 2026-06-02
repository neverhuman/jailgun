# ADR-001: Run History Persistence via JSON Files

## Status

Accepted — 2026-06-01

## Context

The Jailgun dashboard currently shows only the active run snapshot. Once a
run ends and the process restarts, all run data is lost. Users want to see
progress over time: success rates, code growth trends, and deploy cadence.

The project has a `db/` directory with a documented data boundary
(`db/README.md`) requiring migrations, rollback instructions, and lock
impact notes before adding a production schema. This is appropriate for
shared multi-user databases but adds significant overhead for a local
single-user tool.

## Decision

Use file-based JSON persistence in `data/run-history/`:

- Each completed run writes one `run-summary-<run_id>.json` file.
- Files are written atomically (tmp/rename pattern, same as JMCP envelopes).
- Files are read at server startup and on `GET /api/history`.
- The directory is gitignored (JSON files are runtime state, not source).
- No external dependencies (no SQLite, no embedded DB).

## Consequences

**Positive:**
- Zero infrastructure: no database server, no driver dependencies.
- Atomic writes prevent corruption from crashes.
- Files are human-readable and `jq`-friendly for ad-hoc analysis.
- Consistent with the JMCP envelope pattern already in the codebase.
- No migration process needed.

**Negative:**
- No query capability beyond sequential scan of all files.
- No cross-run joins or complex aggregations server-side.
- If hundreds of runs accumulate, `read_run_history` will slow down
  (mitigated by the dashboard only needing the last ~50 runs).

## When to Revisit

- If the tool becomes multi-user or needs concurrent write access.
- If run history exceeds ~500 entries and read performance degrades.
- If complex queries are needed (e.g., "show me all runs where tab 3 failed
  with exit code 1 on a Thursday").

At that point, SQLite via `rusqlite` is the natural next step, with the
formal migration process documented in `db/README.md`.

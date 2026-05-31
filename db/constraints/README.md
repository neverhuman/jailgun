# Constraint Lane

No SQL constraints are committed today. Future durable invariants should prefer
foreign key, check constraint, and row level security proof where applicable,
with tests routed through the migration lane before release.

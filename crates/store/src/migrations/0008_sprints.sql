-- The sprints (docs/SPEC.md 5.5), derived from `sprint.started` and `sprint.ended`: which one is
-- open, and what it may spend. `budget_usd` is NULL for a sprint with no budget of its own
-- (ADR 0015).
CREATE TABLE sprints (
    sprint_id  TEXT PRIMARY KEY,
    budget_usd REAL,
    open       INTEGER NOT NULL CHECK (open IN (0, 1))
) STRICT;
-- The sprint a task is in: set by `sprint.planned`, cleared by the `sprint.ended` that leaves it.
ALTER TABLE task_projections ADD COLUMN sprint TEXT;
-- The sprint a cost's task was in when the cost was recorded, so that a cost stays with the sprint
-- it was spent in after its task leaves.
ALTER TABLE cost_records ADD COLUMN sprint TEXT;
-- As in 0007: the new columns are read back by replaying the log, not by a backfill here.
DELETE FROM sprints;
DELETE FROM task_projections;
DELETE FROM cost_records;
DELETE FROM projection_cursor;

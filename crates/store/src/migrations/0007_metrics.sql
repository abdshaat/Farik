-- What the harness metrics count per contract (docs/SPEC.md F17): how many times it moved into
-- `verifying`, how many into `rejected`, and how many times the human had to act where the process
-- did not ask them to. The rates are computed from these on demand.
ALTER TABLE task_projections ADD COLUMN verifications INTEGER NOT NULL DEFAULT 0
    CHECK (verifications >= 0);
ALTER TABLE task_projections ADD COLUMN rejections INTEGER NOT NULL DEFAULT 0
    CHECK (rejections >= 0);
ALTER TABLE task_projections ADD COLUMN interventions INTEGER NOT NULL DEFAULT 0
    CHECK (interventions >= 0);
-- An older database's counts are read back by replaying the log rather than by a backfill here,
-- which would be a second copy of the projection's rules that could disagree with the first.
-- Emptying the projections, as `rebuild` does, is what makes the next open replay the whole log.
DELETE FROM task_projections;
DELETE FROM cost_records;
DELETE FROM projection_cursor;

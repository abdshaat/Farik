-- Who holds each contract and how many times it has been sent back, derived from
-- `task.transitioned` (docs/SPEC.md 5.2). Like the rest of the board, nothing here is a source of
-- truth. A sprint arrives with phase 4, which has the sprints.
ALTER TABLE task_projections ADD COLUMN assignee_id TEXT;
ALTER TABLE task_projections ADD COLUMN reviewer_id TEXT;
ALTER TABLE task_projections ADD COLUMN iteration INTEGER NOT NULL DEFAULT 0;

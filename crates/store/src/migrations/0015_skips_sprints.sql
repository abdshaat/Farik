-- A request that raises a marketing plan's budget skips the sprint queue (ADR 0042, ADR 0028): a
-- `task.created` with `raises` marks its task, and the sprint policy reads the mark. No event
-- before this one carries the field, so every row starts unmarked and nothing is replayed.
ALTER TABLE task_projections ADD COLUMN skips_sprints INTEGER NOT NULL DEFAULT 0;

-- When a task last began to count towards its assignee's work-in-progress limit (ADR 0062): the
-- sequence number of the event that moved it into a counted status, cleared its last wait on the
-- human, or handed it to another agent. A row made before this column has none, and ranks as 0.
-- Nothing is replayed.
ALTER TABLE task_projections ADD COLUMN counted_seq INTEGER;

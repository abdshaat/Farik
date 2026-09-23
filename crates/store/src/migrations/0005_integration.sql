-- Whether an accepted task's branch has yet to reach the integration branch (docs/SPEC.md 5.14):
-- set by a task's move into `accepted`, cleared by `task.integrated`. An epic has no branch of its
-- own, so it never awaits. A task accepted before this column existed reads as awaiting rather
-- than as integrated, because nothing recorded that it was.
ALTER TABLE task_projections ADD COLUMN awaiting_integration INTEGER NOT NULL DEFAULT 0
    CHECK (awaiting_integration IN (0, 1));
UPDATE task_projections SET awaiting_integration = 1 WHERE status = 'accepted' AND kind = 'task';

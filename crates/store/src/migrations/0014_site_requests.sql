-- How many of a task's site requests wait for the owner to allow or refuse (docs/SPEC.md 5.7 and
-- 6.10; ADR 0039): counted up by `site.requested` and down by a `site.approved` or `site.declined`
-- that names the request. An open request waits on the human as an open question does, so the task
-- is not handed in and no rule counts it as stuck. No log an older Farik wrote holds these events,
-- so nothing is read back.
ALTER TABLE task_projections ADD COLUMN open_sites INTEGER NOT NULL DEFAULT 0
    CHECK (open_sites >= 0);

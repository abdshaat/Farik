-- How many of a task's connector calls wait for the human to allow or refuse them (docs/SPEC.md
-- 5.7; ADR 0031): counted up by `tool_approval.requested` and down by `tool_approval.granted` or
-- `tool_approval.refused`. An open approval waits on the human as an open question does. No log an
-- older Farik wrote holds these events, so nothing is read back.
ALTER TABLE task_projections ADD COLUMN open_approvals INTEGER NOT NULL DEFAULT 0
    CHECK (open_approvals >= 0);

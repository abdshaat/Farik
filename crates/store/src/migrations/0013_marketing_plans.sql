-- How many of a task's marketing plans wait for the owner to approve or send them back
-- (docs/SPEC.md 5.7 and 6.5; ADR 0042): counted up by `marketing_plan.proposed` and down by
-- `marketing_plan.approved` or `marketing_plan.returned`. An open plan waits on the human as an
-- open question does, so the task is not handed in and no rule counts it as stuck. No log an
-- older Catervas wrote holds these events, so nothing is read back.
ALTER TABLE task_projections ADD COLUMN open_plans INTEGER NOT NULL DEFAULT 0
    CHECK (open_plans >= 0);

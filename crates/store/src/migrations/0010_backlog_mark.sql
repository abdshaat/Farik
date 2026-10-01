-- A sprint ended early under the policy "plan work in sprints" (ADR 0028) leaves its unfinished
-- tasks waiting in the Backlog: `sprint.ended` with `backlog: true` marks each task in its `left`,
-- the `sprint.planned` that puts the task in a sprint clears the mark, and a `team.updated` with
-- `plan_in_sprints: false` clears every mark. No event before this one carries either field, so
-- every row starts unmarked and nothing is replayed.
ALTER TABLE task_projections ADD COLUMN left_for_the_backlog INTEGER NOT NULL DEFAULT 0;

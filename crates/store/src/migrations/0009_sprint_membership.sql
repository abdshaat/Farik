-- A task could be left in an ended sprint: a plan applied after its sprint's end, or an end whose
-- `left` missed a task the board held in it. The projections now take a plan into an ended sprint
-- as nothing and an end as taking out every unfinished task in it; emptying them replays the log
-- under those rules, which frees any task a database made before this held in an ended sprint.
DELETE FROM sprints;
DELETE FROM task_projections;
DELETE FROM cost_records;
DELETE FROM projection_cursor;

-- What the team has spent, derived from `cost.recorded` (docs/SPEC.md 5.5).
--
-- One row per event rather than running totals per scope, because the sums wanted are several (by
-- task, agent, session, day, and later purpose) and a task's distinct sessions is a query over rows,
-- not another counter to keep right. Like the board, nothing here is a source of truth.
--
-- `task_id` is NULL for a session with no task, which still costs the day. `agent_id` and
-- `session_id` are nullable only because the event schema cannot require them; the runtime refuses
-- a cost without them, and a row without one is left out of the sums by that key. `day` is the UTC
-- date of `recorded_at`.
CREATE TABLE cost_records (
    seq           INTEGER PRIMARY KEY,
    task_id       TEXT,
    agent_id      TEXT,
    session_id    TEXT,
    day           TEXT NOT NULL,
    purpose       TEXT NOT NULL,
    model_id      TEXT NOT NULL,
    input_tokens  INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cost_usd      REAL NOT NULL
) STRICT;

-- The board reads each task's sum by its id.
CREATE INDEX cost_records_by_task ON cost_records (task_id) WHERE task_id IS NOT NULL;

-- The board, derived from the log (docs/SPEC.md 5.1, 8.4, and 10).
--
-- Nothing here is a source of truth: every row is derivable from the events, and dropping the two
-- tables and replaying the log is always correct. They exist because `docs/SPEC.md` section 10 asks
-- the UI to stay responsive with ten thousand events in a project, which a scan of the log per view
-- is not, and because a view wants one row per contract rather than the history of one.

-- One row per contract, holding what a board shows. The fields a phase 2 event carries and no
-- others: an assignee, a reviewer, a sprint and an iteration arrive with `task.transitioned` in
-- phase 3, and a column nothing can write is a column no test can hold to anything.
CREATE TABLE task_projections (
    task_id     TEXT PRIMARY KEY,
    kind        TEXT NOT NULL,
    parent      TEXT,
    title       TEXT NOT NULL,
    status      TEXT NOT NULL,
    risk        TEXT NOT NULL,
    triaged     INTEGER NOT NULL CHECK (triaged IN (0, 1)),
    locked      INTEGER NOT NULL CHECK (locked IN (0, 1)),
    updated_seq INTEGER NOT NULL
) STRICT;

CREATE INDEX task_projections_by_status ON task_projections (status, task_id);
CREATE INDEX task_projections_by_parent ON task_projections (parent, task_id)
    WHERE parent IS NOT NULL;

-- How far into the log the projections have read. One row, and the `CHECK` is what keeps it one: a
-- second cursor would make "how far" a question with two answers.
CREATE TABLE projection_cursor (
    id  INTEGER PRIMARY KEY CHECK (id = 1),
    seq INTEGER NOT NULL
) STRICT;

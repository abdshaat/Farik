-- The event log and the counter that hands out task ids (docs/SPEC.md 5.1, 8.4).
--
-- Every table is STRICT, so a value SQLite cannot convert to the column's declared type is refused
-- rather than stored as whatever it came in as: a blob is not a team id, and a word is not a
-- counter. STRICT does convert a number to text, so a TEXT column is a promise about what comes
-- back out rather than about what a caller may write. The log is the source of truth for what
-- happened, and a log that accepts anything is not one. `schema_migrations` is not here: the
-- ledger of what has been applied belongs to the applier, which makes it before it reads it.

-- One row per event. The envelope's fields are columns, because they are what the log is queried
-- by; the body is the canonical JSON of docs/schemas/event.schema.json, because its shape belongs
-- to the kind rather than to the table. Nothing is stored twice: a read rebuilds the wire value
-- from the columns and the body and hands it to the protocol crate's reader, so a row that cannot
-- be read back is refused rather than half-built.
CREATE TABLE events (
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    recorded_at TEXT NOT NULL,
    team_id     TEXT NOT NULL,
    project_id  TEXT NOT NULL,
    task_id     TEXT,
    agent_id    TEXT,
    session_id  TEXT,
    kind        TEXT NOT NULL,
    -- A body that is not JSON would make every later read of the log fail, whichever events the
    -- query asked for, because a read hands each row to the protocol crate's reader. Nothing above
    -- this table can write such a row; the engine is what stops everything else.
    body        TEXT NOT NULL CHECK (json_valid(body))
) STRICT;

CREATE INDEX events_by_task ON events (task_id, seq) WHERE task_id IS NOT NULL;
CREATE INDEX events_by_agent ON events (agent_id, seq) WHERE agent_id IS NOT NULL;
CREATE INDEX events_by_kind ON events (kind, seq);

-- Append-only in the engine, not only in the code above it: `farik doctor`, a migration, a repair
-- script, and a person with the sqlite3 shell all go through these.
CREATE TRIGGER events_refuse_update
BEFORE UPDATE ON events
BEGIN
    SELECT RAISE(ABORT, 'the event log is append-only: an event cannot be changed');
END;

CREATE TRIGGER events_refuse_delete
BEFORE DELETE ON events
BEGIN
    SELECT RAISE(ABORT, 'the event log is append-only: an event cannot be deleted');
END;

-- The next number for each task id prefix. One row, `FRK`, until a second prefix exists.
CREATE TABLE task_counters (
    prefix TEXT PRIMARY KEY,
    next   INTEGER NOT NULL
) STRICT;

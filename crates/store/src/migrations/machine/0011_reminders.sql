-- The daily reminder (docs/SPEC.md 4.4, 8.4). Nothing here is read from a conversation, and no
-- to-do text is ever kept. Times are RFC 3339 UTC, as in 0001.

-- How many items of a to-do list a `TodoWrite` call left not completed. Only that tool's calls have
-- one; for every other tool it is NULL.
ALTER TABLE tool_calls ADD COLUMN open_todos INTEGER;

-- The scheduled workflows of the owner's repositories on GitHub, as the last read of GitHub saw
-- them: the workflow's state, and the conclusion and start of its last scheduled run.
CREATE TABLE github_runs (
    full_name       TEXT NOT NULL,
    workflow        TEXT NOT NULL,
    state           TEXT,
    last_conclusion TEXT,
    last_run_at     TEXT,
    PRIMARY KEY (full_name, workflow)
) STRICT;

-- The latest deployment of each of those repositories, and the latest state it reported.
CREATE TABLE github_deploys (
    full_name   TEXT PRIMARY KEY,
    environment TEXT,
    state       TEXT,
    created_at  TEXT
) STRICT;

-- What the owner did with an item of the daily digest, by its key: a snooze hides it until a time, a
-- dismissal while its fingerprint holds.
CREATE TABLE reminder_marks (
    key         TEXT PRIMARY KEY,
    kind        TEXT NOT NULL CHECK (kind IN ('snooze', 'dismiss')),
    until       TEXT,
    fingerprint TEXT,
    at          TEXT NOT NULL
) STRICT;

-- Open to-dos were not read before this version, so every transcript is read again, once. The
-- upserts of 0001 and the inserts of 0003 give the same result when repeated.
UPDATE transcript_files SET read_to = 0, turns = 0, unreadable_lines = 0;

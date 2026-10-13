-- The machine-level index of the owner's Claude Code history (docs/SPEC.md 8.4). It keeps metadata
-- only, never a conversation's text. Times are RFC 3339 UTC, as the project log stamps them.

-- Where reading each transcript file stopped, so a second scan reads only what was appended.
CREATE TABLE transcript_files (
    path             TEXT PRIMARY KEY,
    read_to          INTEGER NOT NULL,
    turns            INTEGER NOT NULL,
    unreadable_lines INTEGER NOT NULL,
    read_at          TEXT NOT NULL
) STRICT;

-- One row per session, merged across every file that mentions it.
CREATE TABLE sessions (
    session_id   TEXT PRIMARY KEY,
    cwd          TEXT,
    git_branch   TEXT,
    version      TEXT,
    entrypoint   TEXT,
    title        TEXT,
    first_at     TEXT,
    last_at      TEXT,
    place        TEXT CHECK (place IN ('owner', 'catervas', 'gone', 'not_a_project')),
    project_root TEXT
) STRICT;

-- One row per model reply, its counters the largest seen: a reply is written as one line per content
-- block, and a resumed session repeats earlier replies.
CREATE TABLE replies (
    reply_key          TEXT PRIMARY KEY,
    session_id         TEXT NOT NULL,
    model              TEXT NOT NULL,
    at                 TEXT NOT NULL,
    input_tokens       INTEGER NOT NULL,
    output_tokens      INTEGER NOT NULL,
    cache_read_tokens  INTEGER NOT NULL,
    cache_write_tokens INTEGER NOT NULL
) STRICT;

CREATE INDEX replies_by_session ON replies (session_id);

-- Every root any scan has placed a session under. Rows are never deleted, so a root seen once stays
-- known.
CREATE TABLE projects (
    root          TEXT PRIMARY KEY,
    is_git        INTEGER NOT NULL,
    ownership     TEXT NOT NULL CHECK (ownership IN ('yours', 'explored')),
    present       INTEGER NOT NULL,
    first_seen_at TEXT NOT NULL,
    checked_at    TEXT NOT NULL
) STRICT;

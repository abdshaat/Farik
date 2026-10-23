-- The owner's claude.ai data export (docs/SPEC.md 8.4): each conversation's id, title, dates and
-- message count, and each Project's id, name, description and file names. Never a message, a
-- document's content, or a person's name or email. Times are RFC 3339 UTC, as in 0001.

-- The exports imported, by the latest time their content holds. An export already imported is
-- ignored.
CREATE TABLE claude_exports (
    account     TEXT NOT NULL,
    content_at  TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    PRIMARY KEY (account, content_at)
) STRICT;

CREATE TABLE claude_conversations (
    uuid               TEXT PRIMARY KEY,
    account            TEXT NOT NULL,
    title              TEXT,
    created_at         TEXT,
    updated_at         TEXT,
    messages           INTEGER NOT NULL,
    seen_in_export_at  TEXT NOT NULL,
    first_imported_at  TEXT NOT NULL
) STRICT;

CREATE TABLE claude_projects (
    uuid               TEXT PRIMARY KEY,
    account            TEXT NOT NULL,
    name               TEXT,
    description        TEXT,
    doc_names          TEXT NOT NULL, -- a JSON array of file names
    created_at         TEXT,
    updated_at         TEXT,
    seen_in_export_at  TEXT NOT NULL,
    first_imported_at  TEXT NOT NULL
) STRICT;

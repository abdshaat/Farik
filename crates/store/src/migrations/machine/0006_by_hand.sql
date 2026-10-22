-- Projects added by hand, and each project's remote (docs/SPEC.md 8.4). A remote is kept as
-- `normalise_remote` writes it: host and path, never a user, a password or a port.

ALTER TABLE projects ADD COLUMN remote TEXT;

-- Keyed by the root, else the remote; the same key again renames.
CREATE TABLE hand_projects (
    key      TEXT PRIMARY KEY,
    name     TEXT NOT NULL,
    root     TEXT,
    remote   TEXT,
    added_at TEXT NOT NULL,
    CHECK (root IS NOT NULL OR remote IS NOT NULL)
) STRICT;

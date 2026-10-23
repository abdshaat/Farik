-- The owner's GitHub repositories pushed to lately, and the branches and pull requests Claude Code
-- on the web made in them (docs/SPEC.md 8.4). Read-only copies of what GitHub said; the token is
-- never here.

CREATE TABLE github_repos (
    full_name      TEXT PRIMARY KEY,
    -- The normalised remote, as a found folder's is, so a repository that is on this computer is one
    -- project with it.
    remote_key     TEXT NOT NULL,
    pushed_at      TEXT NOT NULL,
    private        INTEGER NOT NULL,
    default_branch TEXT NOT NULL,
    -- When a read last saw it. A repository missing from a newer read is kept, marked not seen lately.
    seen_at        TEXT NOT NULL
) STRICT;

CREATE TABLE github_claude_work (
    full_name  TEXT NOT NULL,
    kind       TEXT NOT NULL CHECK (kind IN ('branch', 'pull')),
    name       TEXT NOT NULL,
    number     INTEGER,
    draft      INTEGER,
    updated_at TEXT,
    PRIMARY KEY (full_name, kind, name)
) STRICT;

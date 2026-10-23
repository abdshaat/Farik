-- What the owner said about the projects Catervas found (docs/SPEC.md 8.4). Nothing here is ever
-- read from a conversation: choices, corrections, the services the owner uses and routines typed by
-- hand.

-- The owner's choice for a project, by its key: the normalised remote, else the real path, the
-- connector project's id or the claude.ai uuid. A project with no row shows its suggestion.
CREATE TABLE choices (
    key    TEXT PRIMARY KEY,
    choice TEXT NOT NULL CHECK (choice IN ('team', 'watch', 'ignore')),
    at     TEXT NOT NULL
) STRICT;

-- Two real paths, `a < b`, that the owner said are different projects.
CREATE TABLE separations (
    a TEXT NOT NULL,
    b TEXT NOT NULL,
    PRIMARY KEY (a, b),
    CHECK (a < b)
) STRICT;

-- A folder that moved: its old path's sessions are placed under the new one.
CREATE TABLE moved (
    old TEXT PRIMARY KEY,
    new TEXT NOT NULL
) STRICT;

-- The services the owner says they use with Claude, by the catalog's id; a name with no catalog id
-- is kept under the key `other:<name>` and forms nothing.
CREATE TABLE declared_services (
    service_id TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    at         TEXT NOT NULL
) STRICT;

-- Routines the owner typed in: a name, the project's key when there is one, and a schedule as text.
CREATE TABLE hand_routines (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    project_key TEXT,
    schedule    TEXT NOT NULL,
    at          TEXT NOT NULL
) STRICT;

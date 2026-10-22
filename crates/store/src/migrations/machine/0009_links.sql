-- What the owner said about which found projects work together (docs/SPEC.md 8.4), and the roots a
-- session wrote under. Nothing here is a conversation, and no path of a file is kept.

-- The owner's answer for a pair of projects, by their keys, `a < b`. `proof` is the JSON of the
-- proofs shown when the owner answered, kept so that a No is asked again only when a proof of a
-- kind that was not among them appears.
CREATE TABLE links (
    a      TEXT NOT NULL,
    b      TEXT NOT NULL,
    answer TEXT NOT NULL CHECK (answer IN ('yes', 'no')),
    proof  TEXT,
    at     TEXT NOT NULL,
    PRIMARY KEY (a, b),
    CHECK (a < b)
) STRICT;

-- The project roots a session changed a file under, by a `Write`, `Edit` or `MultiEdit` call. The
-- file itself is read when the transcript is, decided as one of the roots, and never kept.
CREATE TABLE session_roots_written (
    session_id TEXT NOT NULL,
    root       TEXT NOT NULL,
    PRIMARY KEY (session_id, root)
) STRICT;

-- Which root a call wrote under was not kept before this version, so every transcript is read
-- again, once. The upserts of earlier migrations and the inserts above give the same result when
-- repeated, so nothing is counted twice.
UPDATE transcript_files SET read_to = 0, turns = 0, unreadable_lines = 0;

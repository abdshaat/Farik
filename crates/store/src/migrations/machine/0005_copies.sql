-- Sessions that are copies of one another, found by their replies (docs/SPEC.md 8.4). No text is
-- hashed: a reply is told by its time to the second, its model and its four token counters. Both
-- tables are rebuilt in one transaction by each scan.

-- `copy_of`: `session_id` is a copy of `other_id`, which is kept. `maybe_same`: the two are linked
-- but one is in another link too, so neither is dropped; the smaller session id comes first.
CREATE TABLE session_links (
    session_id TEXT NOT NULL,
    other_id   TEXT NOT NULL,
    kind       TEXT NOT NULL CHECK (kind IN ('copy_of', 'maybe_same')),
    PRIMARY KEY (session_id, other_id)
) STRICT;

-- A reply counted once: `reply_key` repeats `same_as`, a reply of a linked session.
CREATE TABLE reply_copies (
    reply_key TEXT PRIMARY KEY,
    same_as   TEXT NOT NULL
) STRICT;

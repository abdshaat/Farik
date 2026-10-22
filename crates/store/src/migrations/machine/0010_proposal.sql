-- What the owner changed on the cards of "Your teams", and what Catervas made when it made them
-- (docs/SPEC.md 8.4). Nothing here is read from a conversation.

-- The owner's edits of a card, by its id (a hash of the keys of the group it came from). A column
-- that is NULL is not edited. `from_claude` is 1 while the name, the purpose or the template came
-- from Claude's suggestion and the owner has not changed them.
CREATE TABLE proposal_edits (
    card_id     TEXT PRIMARY KEY,
    name        TEXT,
    template    TEXT,
    purpose     TEXT,
    from_claude INTEGER NOT NULL DEFAULT 0 CHECK (from_claude IN (0, 1))
) STRICT;

-- A project the owner moved out of a card. It is its own card from then on.
CREATE TABLE moved_out (
    card_id     TEXT NOT NULL,
    project_key TEXT NOT NULL,
    PRIMARY KEY (card_id, project_key)
) STRICT;

-- A team Catervas made from a card, by its slug in the hub. `made` is the JSON of what was
-- created: `.catervas/` in the owner's folder, a project folder, or a new folder, and the keys of
-- the projects it was made from.
CREATE TABLE made_teams (
    slug TEXT PRIMARY KEY,
    root TEXT NOT NULL,
    made TEXT NOT NULL,
    at   TEXT NOT NULL
) STRICT;

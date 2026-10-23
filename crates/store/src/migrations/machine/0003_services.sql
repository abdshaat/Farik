-- The services the owner reaches through Claude's connectors (docs/SPEC.md 8.4). A tool call keeps
-- its id, its tool's name and, for a file tool, whether the file was a spreadsheet; never its input.
-- A configured server keeps its name, kind, host and program name; never a key. Times are RFC 3339
-- UTC, as in 0001.

-- A tool use with a server (`mcp__<server>__<tool>`) or that touched a spreadsheet. Claude Code
-- writes one line per content block, so a reply's tool uses arrive over several lines; the first
-- row of an id stays.
CREATE TABLE tool_calls (
    tool_use_id        TEXT PRIMARY KEY,
    reply_key          TEXT NOT NULL,
    tool_name          TEXT NOT NULL,
    touches_spreadsheet INTEGER NOT NULL CHECK (touches_spreadsheet IN (0, 1)),
    at                 TEXT NOT NULL
) STRICT;

CREATE INDEX tool_calls_by_reply ON tool_calls (reply_key);

-- A snapshot of the connectors Claude's settings name, replaced on each scan. `found_in` is '' for
-- the user level and for claude.ai, else a project path, `plugin:<id>`, a Claude Desktop folder or
-- `extension:<folder name>`.
CREATE TABLE configured_servers (
    source   TEXT NOT NULL CHECK (source IN ('claude_code', 'claude_ai', 'claude_desktop')),
    found_in TEXT NOT NULL,
    name     TEXT NOT NULL,
    kind     TEXT NOT NULL CHECK (kind IN ('stdio', 'http', 'sse')),
    host     TEXT,
    program  TEXT,
    PRIMARY KEY (source, found_in, name)
) STRICT;

-- Tool calls were not kept before this version, so every transcript is read again, once. The
-- upserts of 0001, the session crons of 0002 and the inserts above give the same result when
-- repeated, so nothing is counted twice.
UPDATE transcript_files SET read_to = 0, turns = 0, unreadable_lines = 0;

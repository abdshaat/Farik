-- The owner's recurring jobs (docs/SPEC.md 8.4): a schedule, a name and a project, never a command
-- line, an environment value, a prompt or a key. Times are RFC 3339 UTC, as in 0001.

CREATE TABLE jobs (
    origin       TEXT NOT NULL,
    file         TEXT NOT NULL,
    name         TEXT NOT NULL,
    kind         TEXT NOT NULL CHECK (kind IN (
                     'github_actions', 'vercel', 'kubernetes', 'render', 'terraform',
                     'crontab', 'systemd_user', 'systemd_system', 'launchd', 'task_scheduler',
                     'session_cron', 'saved_session_cron', 'background_job')),
    display_name TEXT NOT NULL,
    schedules    TEXT NOT NULL, -- a JSON array of strings
    folder       TEXT,
    project_root TEXT,
    expires_at   TEXT,
    state        TEXT,
    state_at     TEXT,
    last_seen    TEXT NOT NULL,
    PRIMARY KEY (origin, file, name)
) STRICT;

CREATE INDEX jobs_by_project_root ON jobs (project_root);

-- Tool calls were not read before this version, so every transcript is read again, once. The
-- upserts of 0001 give the same result when repeated, so nothing is counted twice.
UPDATE transcript_files SET read_to = 0, turns = 0, unreadable_lines = 0;

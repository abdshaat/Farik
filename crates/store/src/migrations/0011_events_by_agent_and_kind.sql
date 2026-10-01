-- One agent's events of one kind, newest or oldest first. Every tick asks each agent for its
-- newest chat messages and the sessions it started since (docs/SPEC.md 4.4), and with only
-- `events_by_agent` and `events_by_kind` SQLite picks the kind's index, which walks every chat
-- message in the project for an agent the user never chatted with. Both columns are matched by
-- equality, so the walk is that agent's rows of that kind alone, already in `seq` order.
CREATE INDEX events_by_agent_and_kind ON events (agent_id, kind, seq) WHERE agent_id IS NOT NULL;

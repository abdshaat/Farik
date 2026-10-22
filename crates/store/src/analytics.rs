//! What the analytics read from the machine index (`docs/SPEC.md` 4.4, 8.4; ADR 0059): the owner's
//! own Claude Code usage counted once per reply, their sessions, active hours and tools used most,
//! and the claude.ai conversations of a period.

use std::collections::{BTreeMap, HashMap};

use catervas_core::claude_services::catalog::match_service;
use catervas_core::claude_services::{called_server, server_segment};
use catervas_core::pricing::{PriceTable, Usage, compute_cost_usd};
use chrono::{DateTime, FixedOffset, NaiveDate, Offset as _, Utc};

use crate::error::StoreError;
use crate::machine::{MachineIndex, read_time, stamp};

/// Replies this close together (in seconds) are one stretch of work.
pub const ACTIVE_GAP_SECS: i64 = 5 * 60;

/// How many tools "used most" names.
pub const TOOLS_SHOWN: usize = 3;

/// A period and the computer's offset from UTC, which days are counted in.
#[derive(Debug, Clone, Copy)]
pub struct Window {
    /// The first moment in the period.
    pub from: DateTime<Utc>,
    /// The first moment after it.
    pub until: DateTime<Utc>,
    /// The computer's offset from UTC, in seconds.
    pub utc_offset_secs: i32,
}

/// A team's project folders: the roots its sessions are placed in.
#[derive(Debug, Clone)]
pub struct OwnerGroup {
    /// Whatever the caller keys the group by, such as the team's slug.
    pub key: String,
    /// The project roots of the team and its members.
    pub roots: Vec<String>,
}

/// A tool the owner used, and how many times.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCount {
    /// The tool as the page names it, such as "Editing files" or "Zoho Books".
    pub label: String,
    /// The calls in the period.
    pub calls: u64,
}

/// What the owner did in Claude Code in some projects in a period.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OwnerActivity {
    /// What the replies would cost at the price table, in US dollars.
    pub usd: f64,
    /// The replies, each counted once.
    pub replies: u64,
    /// The replies on a model the table has no price for; their tokens cost nothing here.
    pub unpriced_replies: u64,
    /// The sessions with a reply in the period.
    pub sessions: u64,
    /// Seconds of work: the gaps under five minutes between replies.
    pub active_secs: u64,
    /// The tools used most, most first.
    pub tools: Vec<ToolCount>,
    /// The sessions with a reply on each local day.
    pub sessions_per_day: BTreeMap<NaiveDate, u64>,
}

/// The owner's activity per group, in no group, and in all.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OwnerAnalytics {
    /// Each group's, in the order the groups were given.
    pub groups: Vec<(String, OwnerActivity)>,
    /// The owner's projects that no group holds: watched ones.
    pub watched: OwnerActivity,
    /// Everything of the owner's in the period.
    pub total: OwnerActivity,
}

/// One reply of the owner's, as the index keeps it.
struct OwnerReply {
    key: String,
    session_id: String,
    root: String,
    model: String,
    at: DateTime<Utc>,
    usage: Usage,
}

/// How a built-in tool or a connector's tool is named on the page.
#[must_use]
pub fn tool_label(tool_name: &str) -> String {
    match tool_name {
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => "Editing files".to_string(),
        "Bash" => "Running commands".to_string(),
        "Read" => "Reading files".to_string(),
        "Grep" | "Glob" => "Searching files".to_string(),
        "WebFetch" | "WebSearch" => "Searching the web".to_string(),
        other => match server_segment(other).and_then(called_server) {
            Some((server, _origin)) => match_service(&server, None, None)
                .map_or(server, |service| service.name.to_string()),
            None => other.to_string(),
        },
    }
}

/// The sums of the replies `replies` names, with the tools called in them.
fn activity_of(
    replies: &[&OwnerReply],
    tools: &HashMap<&str, Vec<&str>>,
    offset: FixedOffset,
    prices: &PriceTable,
) -> OwnerActivity {
    let mut activity = OwnerActivity::default();
    let mut sessions: BTreeMap<NaiveDate, std::collections::BTreeSet<&str>> = BTreeMap::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut times: Vec<DateTime<Utc>> = Vec::new();
    for reply in replies {
        activity.replies += 1;
        match compute_cost_usd(&reply.usage, &reply.model, prices) {
            Ok(usd) => activity.usd += usd,
            Err(_) => activity.unpriced_replies += 1,
        }
        seen.insert(reply.session_id.as_str());
        sessions
            .entry(reply.at.with_timezone(&offset).date_naive())
            .or_default()
            .insert(reply.session_id.as_str());
        times.push(reply.at);
        for tool in tools.get(reply.key.as_str()).into_iter().flatten() {
            *counts.entry(tool_label(tool)).or_default() += 1;
        }
    }
    activity.sessions = seen.len() as u64;
    activity.sessions_per_day = sessions
        .into_iter()
        .map(|(day, ids)| (day, ids.len() as u64))
        .collect();
    times.sort();
    activity.active_secs = times
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).num_seconds())
        .filter(|gap| *gap < ACTIVE_GAP_SECS)
        .map(i64::unsigned_abs)
        .sum();
    let mut counted: Vec<ToolCount> = counts
        .into_iter()
        .map(|(label, calls)| ToolCount { label, calls })
        .collect();
    counted.sort_by(|a, b| b.calls.cmp(&a.calls).then_with(|| a.label.cmp(&b.label)));
    counted.truncate(TOOLS_SHOWN);
    activity.tools = counted;
    activity
}

impl MachineIndex {
    /// The owner's activity in `window`, for each group of project roots and for the rest. Catervas's
    /// own agents' sessions and the replies another session copied are not counted.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index cannot be read.
    pub fn owner_analytics(
        &self,
        groups: &[OwnerGroup],
        window: &Window,
        prices: &PriceTable,
    ) -> Result<OwnerAnalytics, StoreError> {
        let offset = FixedOffset::east_opt(window.utc_offset_secs).unwrap_or_else(|| Utc.fix());
        let replies = self.owner_replies(window)?;
        let tools = self.owner_tools(window)?;
        let by_reply: HashMap<&str, Vec<&str>> = {
            let mut map: HashMap<&str, Vec<&str>> = HashMap::new();
            for (reply, tool) in &tools {
                map.entry(reply.as_str()).or_default().push(tool.as_str());
            }
            map
        };
        let everything: Vec<&OwnerReply> = replies.iter().collect();
        let mut result = OwnerAnalytics {
            total: activity_of(&everything, &by_reply, offset, prices),
            ..OwnerAnalytics::default()
        };
        for group in groups {
            let mine: Vec<&OwnerReply> = replies
                .iter()
                .filter(|reply| group.roots.contains(&reply.root))
                .collect();
            result.groups.push((
                group.key.clone(),
                activity_of(&mine, &by_reply, offset, prices),
            ));
        }
        let rest: Vec<&OwnerReply> = replies
            .iter()
            .filter(|reply| !groups.iter().any(|group| group.roots.contains(&reply.root)))
            .collect();
        result.watched = activity_of(&rest, &by_reply, offset, prices);
        Ok(result)
    }

    /// The owner's replies in the period that sit in a project, each once.
    fn owner_replies(&self, window: &Window) -> Result<Vec<OwnerReply>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT r.reply_key, r.session_id, s.project_root, r.model, r.at,
                    r.input_tokens, r.output_tokens, r.cache_read_tokens, r.cache_write_tokens
             FROM replies r JOIN sessions s ON s.session_id = r.session_id
             WHERE s.place = 'owner' AND s.project_root IS NOT NULL
               AND r.at >= ?1 AND r.at < ?2
               AND r.reply_key NOT IN (SELECT reply_key FROM reply_copies)
             ORDER BY r.at, r.reply_key",
        )?;
        let mut rows = statement.query((stamp(window.from), stamp(window.until)))?;
        let mut replies = Vec::new();
        while let Some(row) = rows.next()? {
            let count = |column: usize| -> rusqlite::Result<u64> {
                Ok(u64::try_from(row.get::<_, i64>(column)?).unwrap_or(0))
            };
            replies.push(OwnerReply {
                key: row.get(0)?,
                session_id: row.get(1)?,
                root: row.get(2)?,
                model: row.get(3)?,
                at: read_time(&row.get::<_, String>(4)?)?,
                usage: Usage {
                    input_tokens: count(5)?,
                    output_tokens: count(6)?,
                    cache_read_tokens: count(7)?,
                    cache_write_tokens: count(8)?,
                },
            });
        }
        Ok(replies)
    }

    /// The `(reply key, tool name)` of every tool call of the owner's replies in the period.
    fn owner_tools(&self, window: &Window) -> Result<Vec<(String, String)>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT t.reply_key, t.tool_name
             FROM tool_calls t JOIN replies r ON r.reply_key = t.reply_key
             WHERE r.at >= ?1 AND r.at < ?2",
        )?;
        let rows = statement
            .query_map((stamp(window.from), stamp(window.until)), |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// The claude.ai conversations active in the period: updated in it, or made in it when never
    /// updated.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index cannot be read.
    pub fn claude_ai_conversations(&self, window: &Window) -> Result<u64, StoreError> {
        let count: i64 = self.connection.query_row(
            "SELECT count(*) FROM claude_conversations
             WHERE coalesce(updated_at, created_at) >= ?1 AND coalesce(updated_at, created_at) < ?2",
            (stamp(window.from), stamp(window.until)),
            |row| row.get(0),
        )?;
        Ok(u64::try_from(count).unwrap_or(0))
    }
}

/// Rows for a test of something that reads the index (`docs/standards/code.md`: fixtures are
/// public so that other crates' tests can use them).
pub mod fixtures {
    use chrono::{DateTime, Utc};

    use crate::machine::{MachineIndex, stamp};

    /// A session placed `place` (`owner`, `catervas`, ...) in the project at `root`.
    ///
    /// # Panics
    ///
    /// When the index refuses the row, which is the fixture's own bug.
    pub fn seed_session(index: &MachineIndex, id: &str, place: &str, root: &str) {
        index
            .connection
            .execute(
                "INSERT INTO sessions (session_id, place, project_root, first_at) VALUES (?1, ?2, ?3, ?4)",
                (id, place, root, stamp(DateTime::UNIX_EPOCH)),
            )
            .expect("a session");
    }

    /// A reply of `session` at `at`, with `input_tokens` input tokens and no others.
    ///
    /// # Panics
    ///
    /// When the index refuses the row, which is the fixture's own bug.
    pub fn seed_reply(
        index: &MachineIndex,
        key: &str,
        session: &str,
        model: &str,
        at: DateTime<Utc>,
        input_tokens: i64,
    ) {
        index
            .connection
            .execute(
                "INSERT INTO replies (reply_key, session_id, model, at, input_tokens, output_tokens,
                                      cache_read_tokens, cache_write_tokens)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, 0)",
                (key, session, model, stamp(at), input_tokens),
            )
            .expect("a reply");
    }

    /// A claude.ai conversation last updated at `updated_at`.
    ///
    /// # Panics
    ///
    /// When the index refuses the row, which is the fixture's own bug.
    pub fn seed_conversation(index: &MachineIndex, uuid: &str, updated_at: DateTime<Utc>) {
        index
            .connection
            .execute(
                "INSERT INTO claude_conversations
                     (uuid, account, updated_at, messages, seen_in_export_at, first_imported_at)
                 VALUES (?1, 'x', ?2, 2, ?2, ?2)",
                (uuid, stamp(updated_at)),
            )
            .expect("a conversation");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::fixtures::TempProject;
    use crate::machine::{MACHINE_DB, open_machine_index};
    use chrono::TimeZone;
    use rusqlite::Connection;

    fn at(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, hour, minute, 0)
            .single()
            .expect("a valid time")
    }

    fn week() -> Window {
        Window {
            from: at(19, 0, 0),
            until: at(26, 0, 0),
            utc_offset_secs: 0,
        }
    }

    fn seed_session(connection: &Connection, id: &str, place: &str, root: &str) {
        connection
            .execute(
                "INSERT INTO sessions (session_id, place, project_root, first_at) VALUES (?1, ?2, ?3, ?4)",
                (id, place, root, stamp(at(19, 0, 0))),
            )
            .expect("a session");
    }

    fn seed_reply(
        connection: &Connection,
        key: &str,
        session: &str,
        model: &str,
        when: DateTime<Utc>,
    ) {
        connection
            .execute(
                "INSERT INTO replies (reply_key, session_id, model, at, input_tokens, output_tokens,
                                      cache_read_tokens, cache_write_tokens)
                 VALUES (?1, ?2, ?3, ?4, 1000000, 100000, 0, 0)",
                (key, session, model, stamp(when)),
            )
            .expect("a reply");
    }

    fn seed_call(connection: &Connection, id: &str, reply: &str, tool: &str, when: DateTime<Utc>) {
        connection
            .execute(
                "INSERT INTO tool_calls (tool_use_id, reply_key, tool_name, touches_spreadsheet, at)
                 VALUES (?1, ?2, ?3, 0, ?4)",
                (id, reply, tool, stamp(when)),
            )
            .expect("a tool call");
    }

    #[test]
    fn counts_the_owner_s_usage_once_per_reply() {
        let temp = TempProject::new("owner-analytics");
        let index = open_machine_index(&temp.root.join(MACHINE_DB), at(22, 9, 0)).expect("opens");
        let c = &index.connection;
        seed_session(c, "s1", "owner", "/a");
        // A resumed session repeats three replies: the index keeps each once, under the session that
        // recorded it, so the resumed session has the same keys and adds none.
        seed_session(c, "s1-resumed", "owner", "/a");
        seed_session(c, "s2", "owner", "/b");
        seed_session(c, "agent", "catervas", "/a");
        // Project A: three replies on a priced model (4 + 2 = $6 each at $4 and $20 per million) ...
        seed_reply(c, "r1", "s1", "claude-opus-5-5", at(20, 10, 0));
        seed_reply(c, "r2", "s1", "claude-opus-5-5", at(20, 10, 3));
        seed_reply(c, "r3", "s1", "claude-opus-5-5", at(20, 10, 20));
        // ... one on an unknown model, and one outside the week.
        seed_reply(c, "r4", "s1-resumed", "claude-unheard-of", at(21, 8, 0));
        seed_reply(c, "r5", "s1", "claude-opus-5-5", at(27, 8, 0));
        // The agent's session is excluded; project B's reply is the watched one.
        seed_reply(c, "r6", "agent", "claude-opus-5-5", at(20, 10, 1));
        seed_reply(c, "r7", "s2", "claude-opus-5-5", at(22, 8, 0));
        // A reply another session copied is not counted again.
        seed_reply(c, "r8", "s2", "claude-opus-5-5", at(22, 8, 1));
        c.execute(
            "INSERT INTO reply_copies (reply_key, same_as) VALUES ('r8', 'r7')",
            [],
        )
        .expect("a copy");
        seed_call(c, "t1", "r1", "Edit", at(20, 10, 0));
        seed_call(c, "t2", "r2", "Write", at(20, 10, 3));
        seed_call(c, "t3", "r3", "Bash", at(20, 10, 20));
        seed_call(c, "t4", "r3", "mcp__zoho_books__list", at(20, 10, 20));

        let groups = [OwnerGroup {
            key: "a".to_string(),
            roots: vec!["/a".to_string()],
        }];
        let found = index
            .owner_analytics(
                &groups,
                &week(),
                &catervas_core::pricing::prices::PRICE_TABLE,
            )
            .expect("analytics");
        let a = &found.groups[0].1;
        assert_eq!(found.groups[0].0, "a");
        assert!(
            (a.usd - 18.0).abs() < 1e-9,
            "three priced replies: {}",
            a.usd
        );
        assert_eq!((a.replies, a.unpriced_replies, a.sessions), (4, 1, 2));
        assert_eq!(a.active_secs, 180, "10:00 to 10:03 only");
        assert_eq!(
            a.tools[0],
            ToolCount {
                label: "Editing files".to_string(),
                calls: 2
            }
        );
        assert_eq!(a.tools.len(), 3);
        assert_eq!(
            a.sessions_per_day
                .get(&NaiveDate::from_ymd_opt(2026, 10, 20).unwrap()),
            Some(&1)
        );
        assert!(
            (found.watched.usd - 6.0).abs() < 1e-9,
            "B: {}",
            found.watched.usd
        );
        assert_eq!(found.total.replies, 5);
        for (uuid, updated) in [("c1", at(21, 9, 0)), ("c2", at(14, 9, 0))] {
            c.execute(
                "INSERT INTO claude_conversations (uuid, account, updated_at, messages, seen_in_export_at, first_imported_at)
                 VALUES (?1, 'x', ?2, 2, ?2, ?2)",
                (uuid, stamp(updated)),
            )
            .expect("a conversation");
        }
        assert_eq!(index.claude_ai_conversations(&week()).expect("counted"), 1);
    }
}

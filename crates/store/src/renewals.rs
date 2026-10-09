//! The renewals Farik flagged for the owner (`docs/SPEC.md` 6.10, ADR 0039), folded from the
//! three `renewal.` kinds of the project's log.

use chrono::{DateTime, NaiveDate, Utc};
use farik_protocol::event::{EventBody, EventKind};

use crate::{EventLog, EventQuery, StoreError};

/// One renewal Farik flagged, and whether the owner dismissed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenewalRecord {
    /// The renewal's number, the seq of its `renewal.flagged`.
    pub renewal: u64,
    /// The vendor, as the register wrote it. Untrusted text.
    pub vendor: String,
    /// The day it renews.
    pub renews_on: NaiveDate,
    /// The last day to change or cancel it.
    pub decide_by: NaiveDate,
    /// When Farik flagged it.
    pub flagged_at: DateTime<Utc>,
    /// Whether the owner dismissed it, by "Dismiss" or by sending a request to review it.
    pub dismissed: bool,
}

/// Every renewal the log holds, oldest first. A dismissal counts only when its envelope names no
/// agent and no session, since only the owner dismisses one.
///
/// # Errors
///
/// What the log refused.
pub fn renewals(log: &EventLog) -> Result<Vec<RenewalRecord>, StoreError> {
    let events = log.read(&EventQuery {
        kinds: vec![EventKind::RenewalFlagged, EventKind::RenewalDismissed],
        ..EventQuery::default()
    })?;
    let mut all: Vec<RenewalRecord> = Vec::new();
    for event in &events {
        let ids = &event.envelope.ids;
        match &event.body {
            EventBody::RenewalFlagged(body) => all.push(RenewalRecord {
                renewal: event.envelope.seq,
                vendor: body.vendor.to_string(),
                renews_on: body.renews_on,
                decide_by: body.decide_by,
                flagged_at: event.envelope.recorded_at,
                dismissed: false,
            }),
            EventBody::RenewalDismissed(body)
                if ids.agent_id.is_none() && ids.session_id.is_none() =>
            {
                if let Some(record) = all
                    .iter_mut()
                    .find(|record| record.renewal == body.renewal.get())
                {
                    record.dismissed = true;
                }
            }
            _ => {}
        }
    }
    Ok(all)
}

/// What a day's run of the check found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenewalCheck {
    /// When it ran.
    pub at: DateTime<Utc>,
    /// How many renewals it flagged.
    pub due: u32,
    /// How many rows it could not read.
    pub unreadable: u32,
}

/// The latest run of the daily check, if there has been one.
///
/// # Errors
///
/// What the log refused.
pub fn last_check(log: &EventLog) -> Result<Option<RenewalCheck>, StoreError> {
    let events = log.read(&EventQuery {
        kinds: vec![EventKind::RenewalChecked],
        ..EventQuery::default()
    })?;
    Ok(events.iter().rev().find_map(|event| match &event.body {
        EventBody::RenewalChecked(body) => Some(RenewalCheck {
            at: event.envelope.recorded_at,
            due: u32::try_from(body.due).unwrap_or(u32::MAX),
            unreadable: u32::try_from(body.unreadable).unwrap_or(u32::MAX),
        }),
        _ => None,
    }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{last_check, renewals};
    use crate::waiting::fixtures::{Board, at};

    fn flagged(board: &Board, minute: u32, vendor: &str, renews_on: &str) -> u64 {
        board
            .put(
                at(9, minute),
                None,
                None,
                "renewal.flagged",
                json!({ "vendor": vendor, "renews_on": renews_on, "decide_by": "2026-10-31" }),
            )
            .envelope
            .seq
    }

    #[test]
    fn the_open_renewals_are_the_undismissed() {
        let board = Board::new("renewals-fold");
        let first = flagged(&board, 1, "Vercel", "2026-11-30");
        let second = flagged(&board, 2, "Notion", "2026-12-15");
        let all = renewals(&board.log).expect("the store reads");
        assert_eq!(
            all.iter()
                .map(|one| (one.renewal, one.dismissed))
                .collect::<Vec<_>>(),
            [(first, false), (second, false)]
        );
        assert_eq!(all[0].vendor, "Vercel");
        assert_eq!(all[0].renews_on.to_string(), "2026-11-30");
        assert_eq!(all[0].decide_by.to_string(), "2026-10-31");
        assert_eq!(all[0].flagged_at, at(9, 1));

        // The owner's dismissal closes it.
        board.put(
            at(9, 3),
            None,
            None,
            "renewal.dismissed",
            json!({ "renewal": first }),
        );
        // An agent's, in or out of a session, closes nothing.
        board.session(
            at(9, 4),
            None,
            "ivo",
            "session-1",
            "renewal.dismissed",
            json!({ "renewal": second }),
        );
        board.put(
            at(9, 5),
            None,
            Some("ivo"),
            "renewal.dismissed",
            json!({ "renewal": second }),
        );
        board.put_with(
            at(9, 5),
            None,
            None,
            Some("session-1"),
            "renewal.dismissed",
            json!({ "renewal": second }),
        );
        let all = renewals(&board.log).expect("the store reads");
        assert_eq!(
            all.iter()
                .map(|one| (one.renewal, one.dismissed))
                .collect::<Vec<_>>(),
            [(first, true), (second, false)]
        );
        // A dismissal of a number that is no renewal is no one's.
        board.put(
            at(9, 6),
            None,
            None,
            "renewal.dismissed",
            json!({ "renewal": 999 }),
        );
        assert_eq!(renewals(&board.log).expect("the store reads").len(), 2);
    }

    #[test]
    fn the_last_check_is_the_latest_run() {
        let board = Board::new("renewals-check");
        assert_eq!(last_check(&board.log).expect("the store reads"), None);
        board.put(
            at(9, 1),
            None,
            None,
            "renewal.checked",
            json!({ "due": 1, "unreadable": 2 }),
        );
        board.put(
            at(9, 2),
            None,
            None,
            "renewal.checked",
            json!({ "due": 0, "unreadable": 3 }),
        );
        let last = last_check(&board.log)
            .expect("the store reads")
            .expect("a run");
        assert_eq!((last.due, last.unreadable, last.at), (0, 3, at(9, 2)));
    }
}

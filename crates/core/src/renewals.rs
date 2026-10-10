//! The renewals a register of vendors says are coming up (`docs/SPEC.md` 6.10, ADR 0039): which
//! rows are due to be put on Today, and how many rows hold a date Catervas cannot read. Pure: the
//! caller reads the sheet and passes each row's four cells as text, and today's date.

use std::collections::BTreeSet;

use chrono::{Duration, NaiveDate};

/// How many days before its decision date a renewal is put on Today.
const WARNING_DAYS: i64 = 14;
/// The most characters a vendor's name has.
const MOST_VENDOR: usize = 100;
/// The most days of notice a renewal has.
const MOST_NOTICE: u32 = 365;

/// One row of the register's `Vendors` sheet, each cell as its text: a date cell as its ISO date,
/// a whole number as its digits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterRow {
    /// The vendor's name.
    pub vendor: String,
    /// The day the vendor renews, ISO.
    pub renews_on: String,
    /// The days of notice the vendor needs, blank for none.
    pub notice_days: String,
    /// Whether the vendor is used now: `active` and `trial` are read.
    pub status: String,
}

/// A renewal that is due.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueRenewal {
    /// The vendor's name, trimmed.
    pub vendor: String,
    /// The day it renews.
    pub renews_on: NaiveDate,
    /// The last day to change or cancel it: `renews_on` less the notice.
    pub decide_by: NaiveDate,
}

/// The renewals due on `today`, and how many rows could not be read.
///
/// Only a row whose `status` is `active` or `trial` (trimmed, without regard to case) is read. In
/// a row read, a blank `renews_on` is no renewal and is not counted; a `renews_on` that is not an
/// ISO date, a `notice_days` neither blank nor a whole number from 0 to 365, and a `vendor` that
/// is blank, longer than 100 characters or holds a control character are each counted, once per
/// row, and never guessed. A renewal is due from 14 days before its decision date to its day,
/// inclusive, unless `flagged` already holds its vendor (without regard to case) and day, and one
/// vendor's one day is reported once.
#[must_use]
pub fn due_renewals(
    rows: &[RegisterRow],
    today: NaiveDate,
    flagged: &[(String, NaiveDate)],
) -> (Vec<DueRenewal>, u32) {
    let mut seen: BTreeSet<(String, NaiveDate)> = flagged
        .iter()
        .map(|(vendor, day)| (vendor.to_lowercase(), *day))
        .collect();
    let mut due = Vec::new();
    let mut unreadable = 0_u32;
    for row in rows {
        let status = row.status.trim().to_lowercase();
        if status != "active" && status != "trial" {
            continue;
        }
        if row.renews_on.trim().is_empty() {
            continue;
        }
        let (Some(renews_on), Some(notice), Some(vendor)) = (
            iso_date(&row.renews_on),
            notice_days(&row.notice_days),
            vendor_name(&row.vendor),
        ) else {
            unreadable += 1;
            continue;
        };
        let decide_by = renews_on - Duration::days(i64::from(notice));
        let from = decide_by - Duration::days(WARNING_DAYS);
        if today < from || today > renews_on {
            continue;
        }
        if seen.insert((vendor.to_lowercase(), renews_on)) {
            due.push(DueRenewal {
                vendor: vendor.to_string(),
                renews_on,
                decide_by,
            });
        }
    }
    (due, unreadable)
}

/// A date written `YYYY-MM-DD`, as a person or a spreadsheet program wrote it.
fn iso_date(cell: &str) -> Option<NaiveDate> {
    let text = cell.trim();
    if text.len() != 10 {
        return None;
    }
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
}

/// The days of notice: blank is none, otherwise digits alone, from 0 to 365.
fn notice_days(cell: &str) -> Option<u32> {
    let text = cell.trim();
    if text.is_empty() {
        return Some(0);
    }
    if !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse::<u32>().ok().filter(|days| *days <= MOST_NOTICE)
}

/// A vendor's name on one line, from 1 to 100 characters.
fn vendor_name(cell: &str) -> Option<&str> {
    let name = cell.trim();
    let length = name.chars().count();
    if length == 0 || length > MOST_VENDOR || name.chars().any(char::is_control) {
        return None;
    }
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").expect("a date")
    }

    fn a_row(vendor: &str, renews_on: &str, notice_days: &str, status: &str) -> RegisterRow {
        RegisterRow {
            vendor: vendor.to_string(),
            renews_on: renews_on.to_string(),
            notice_days: notice_days.to_string(),
            status: status.to_string(),
        }
    }

    fn due_on(rows: &[RegisterRow], today: &str) -> (Vec<DueRenewal>, u32) {
        due_renewals(rows, day(today), &[])
    }

    #[test]
    fn due_two_weeks_before_the_decision_date() {
        let rows = [a_row("Vercel", "2026-11-30", "30", "active")];
        let mut today = day("2026-10-17");
        while today <= day("2026-11-30") {
            let (due, unreadable) = due_renewals(&rows, today, &[]);
            assert_eq!(unreadable, 0, "{today}");
            assert_eq!(due.len(), 1, "{today}");
            assert_eq!(due[0].vendor, "Vercel");
            assert_eq!(due[0].renews_on, day("2026-11-30"));
            assert_eq!(due[0].decide_by, day("2026-10-31"));
            today = today.succ_opt().expect("a next day");
        }
        assert!(due_on(&rows, "2026-10-16").0.is_empty());
        assert!(due_on(&rows, "2026-12-01").0.is_empty());

        // A blank notice is no days: the decision date is the renewal day itself.
        let blank = [a_row("Vercel", "2026-11-30", "", "active")];
        let (due, _) = due_on(&blank, "2026-11-16");
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].decide_by, day("2026-11-30"));
        assert!(due_on(&blank, "2026-11-15").0.is_empty());
    }

    #[test]
    fn only_an_active_or_trial_row_is_read() {
        let today = "2026-11-20";
        for status in [" Active ", "TRIAL", "active", "trial"] {
            let rows = [a_row("Vercel", "2026-11-30", "30", status)];
            assert_eq!(due_on(&rows, today).0.len(), 1, "{status:?}");
        }
        for status in ["cancelled", "planned", "", "inactive", "actively"] {
            // A date nobody can read is not counted either: the row is not read at all.
            for (renews_on, notice) in [("2026-11-30", "30"), ("next month", "ten")] {
                let rows = [a_row("Vercel", renews_on, notice, status)];
                assert_eq!(due_on(&rows, today), (vec![], 0), "{status:?} {renews_on}");
            }
        }
    }

    #[test]
    fn once_per_vendor_and_date() {
        // The log keeps the vendor as it was flagged; the register may spell it another way.
        let flagged = [("Vercel".to_string(), day("2026-11-30"))];
        let rows = [a_row("vercel", "2026-11-30", "0", "active")];
        assert!(
            due_renewals(&rows, day("2026-11-20"), &flagged)
                .0
                .is_empty()
        );

        let later = [a_row("Vercel", "2026-12-31", "0", "active")];
        assert_eq!(due_renewals(&later, day("2026-12-20"), &flagged).0.len(), 1);

        let twice = [
            a_row("Vercel", "2026-11-30", "0", "active"),
            a_row("VERCEL", "2026-11-30", "0", "trial"),
        ];
        assert_eq!(due_on(&twice, "2026-11-20").0.len(), 1);
    }

    #[test]
    fn an_unreadable_row_is_counted_not_guessed() {
        let long = "v".repeat(101);
        let fits = "v".repeat(100);
        let unreadable = [
            a_row("Vercel", "next month", "30", "active"),
            a_row("Vercel", "30/11/2026", "30", "active"),
            a_row("Vercel", "2026-13-01", "30", "active"),
            a_row("Vercel", "2026-11-30", "-3", "active"),
            a_row("Vercel", "2026-11-30", "366", "active"),
            a_row("Vercel", "2026-11-30", "ten", "active"),
            a_row("Vercel", "2026-11-30", "2.5", "active"),
            a_row("Vercel", "2026-11-30", "+30", "active"),
            a_row("Vercel", "2026-1-5", "30", "active"),
            a_row("", "2026-11-30", "30", "active"),
            a_row("   ", "2026-11-30", "30", "active"),
            a_row(&long, "2026-11-30", "30", "active"),
            a_row("A\nB", "2026-11-30", "30", "active"),
            a_row("A\u{7}B", "2026-11-30", "30", "active"),
        ];
        for row in &unreadable {
            assert_eq!(
                due_on(std::slice::from_ref(row), "2026-11-20"),
                (vec![], 1),
                "{row:?}"
            );
        }
        // Every fault in one row is still one row.
        let all_wrong = [a_row("", "soon", "ten", "active")];
        assert_eq!(due_on(&all_wrong, "2026-11-20"), (vec![], 1));
        let (due, count) = due_on(&unreadable, "2026-11-20");
        assert!(due.is_empty());
        assert_eq!(count, 14);

        // A blank renewal date is no renewal: neither due nor counted.
        let blank = [a_row("Vercel", "  ", "30", "active")];
        assert_eq!(due_on(&blank, "2026-11-20"), (vec![], 0));
        // The longest vendor that fits is read.
        let fits = [a_row(&fits, "2026-11-30", "0", "active")];
        assert_eq!(due_on(&fits, "2026-11-20").0.len(), 1);
    }
}

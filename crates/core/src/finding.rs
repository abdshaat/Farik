//! What to suggest for each project the machine index found, and where its row sits in the list
//! (`docs/SPEC.md` 4.1; ADR 0059). Pure: the choice itself is the owner's and is kept by the store.

use chrono::{DateTime, Duration, Utc};

/// How many days back a project counts as worked on lately.
pub const RECENT_DAYS: i64 = 30;

/// What the owner can say about a found project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Make a team for it.
    Team,
    /// Keep an eye on it and make nothing.
    Watch,
    /// Leave it out.
    Ignore,
}

impl Choice {
    /// The choice as it is written on the wire and in the index.
    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Team => "team",
            Self::Watch => "watch",
            Self::Ignore => "ignore",
        }
    }

    /// The choice a wire word names.
    #[must_use]
    pub fn from_wire(wire: &str) -> Option<Self> {
        match wire {
            "team" => Some(Self::Team),
            "watch" => Some(Self::Watch),
            "ignore" => Some(Self::Ignore),
            _ => None,
        }
    }
}

/// What a row of the found list is, as far as a suggestion goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Candidate {
    /// A folder or repository of the owner's own, last worked in at `last_at`.
    Yours {
        /// When any session in it last did anything.
        last_at: DateTime<Utc>,
    },
    /// A repository the owner is only looking at.
    Explored,
    /// A project formed from connector work, last used at `last_at`.
    Connector {
        /// When any of its sessions last did anything.
        last_at: DateTime<Utc>,
    },
    /// A claude.ai Project.
    ClaudeAi,
    /// A job on this computer that belongs to no project.
    ComputerJob,
    /// A folder that is not on this computer any more.
    Gone,
    /// A project the owner added by hand.
    ByHand,
    /// A repository on the owner's GitHub that no folder on this computer has.
    Github,
}

/// The list's sections a row can sit in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// The rows the owner is shown first, newest first.
    Main,
    /// Repositories the owner is only looking at.
    Explored,
    /// Folders that are gone.
    Gone,
    /// Jobs on this computer with no project.
    ComputerJobs,
}

impl Section {
    /// The section as it is written on the wire.
    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Explored => "explored",
            Self::Gone => "gone",
            Self::ComputerJobs => "computer_jobs",
        }
    }
}

fn is_recent(last_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    now - last_at <= Duration::days(RECENT_DAYS)
}

/// What Catervas suggests for `candidate` at `now`; `None` for a gone folder, which is only
/// located, never offered.
#[must_use]
pub fn suggest(candidate: Candidate, now: DateTime<Utc>) -> Option<Choice> {
    match candidate {
        Candidate::Yours { last_at } | Candidate::Connector { last_at } => {
            Some(if is_recent(last_at, now) {
                Choice::Team
            } else {
                Choice::Watch
            })
        }
        Candidate::Explored => Some(Choice::Ignore),
        Candidate::ClaudeAi | Candidate::ComputerJob | Candidate::Github => Some(Choice::Watch),
        Candidate::ByHand => Some(Choice::Team),
        Candidate::Gone => None,
    }
}

/// The section a row sits in.
#[must_use]
pub fn section_of(candidate: Candidate) -> Section {
    match candidate {
        Candidate::Explored => Section::Explored,
        Candidate::Gone => Section::Gone,
        Candidate::ComputerJob => Section::ComputerJobs,
        Candidate::Yours { .. }
        | Candidate::Connector { .. }
        | Candidate::ClaudeAi
        | Candidate::ByHand
        | Candidate::Github => Section::Main,
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn suggests_a_choice_for_each_project() {
        let now = Utc
            .with_ymd_and_hms(2026, 10, 22, 12, 0, 0)
            .single()
            .expect("a valid time");
        let ago = |days| now - Duration::days(days);
        let cases = [
            (Candidate::Yours { last_at: ago(29) }, Some(Choice::Team)),
            (Candidate::Yours { last_at: ago(31) }, Some(Choice::Watch)),
            (Candidate::Explored, Some(Choice::Ignore)),
            (
                Candidate::Connector { last_at: ago(29) },
                Some(Choice::Team),
            ),
            (
                Candidate::Connector { last_at: ago(31) },
                Some(Choice::Watch),
            ),
            (Candidate::ClaudeAi, Some(Choice::Watch)),
            (Candidate::ComputerJob, Some(Choice::Watch)),
            (Candidate::ByHand, Some(Choice::Team)),
            (Candidate::Gone, None),
        ];
        for (candidate, wanted) in cases {
            assert_eq!(suggest(candidate, now), wanted, "{candidate:?}");
        }
        assert_eq!(section_of(Candidate::Explored), Section::Explored);
        assert_eq!(section_of(Candidate::Gone), Section::Gone);
        assert_eq!(section_of(Candidate::ComputerJob), Section::ComputerJobs);
        assert_eq!(section_of(Candidate::ClaudeAi), Section::Main);
        assert_eq!(Choice::from_wire("watch"), Some(Choice::Watch));
    }
}

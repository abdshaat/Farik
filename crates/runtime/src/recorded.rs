//! Sessions replayed from transcripts the Claude Code program once printed, so that everything
//! above the runtime is tested without the program, the network, or money.

pub mod fixtures;

/// The lines of one recorded `stream-json` session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    lines: Vec<String>,
}

impl Transcript {
    /// A transcript from JSON-lines text; blank lines are passed over.
    #[must_use]
    pub fn from_jsonl(text: &str) -> Transcript {
        Transcript {
            lines: text
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(str::to_string)
                .collect(),
        }
    }

    /// Its lines, in the order the program printed them.
    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().map(String::as_str)
    }
}

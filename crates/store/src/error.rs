//! What the store refuses, and why.

use std::fmt;

/// Why a store operation did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// The file system refused: the directory could not be made, the file could not be reached.
    Io {
        /// What failed, in the words the operating system used.
        detail: String,
    },
    /// SQLite refused: a statement failed, the database is locked, a migration did not apply.
    Sqlite {
        /// What failed, in SQLite's own words.
        detail: String,
    },
    /// A row of the log cannot be read back as an event. The log is append-only and every append
    /// goes through the protocol crate's rules, so this means the file was changed by something
    /// else, or was written by a version of Farik this one does not understand.
    InvalidEvent {
        /// Which row and what is wrong with it.
        detail: String,
    },
    /// The task id counter has passed what the contract schema's pattern can spell
    /// (`^FRK-[0-9]{1,6}$`), so the store has no id left to hand out. Refused rather than
    /// returning something that is not a task id.
    TaskIdsExhausted {
        /// The number the counter reached.
        next: u64,
    },
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { detail } => write!(formatter, "the file system refused: {detail}"),
            Self::Sqlite { detail } => write!(formatter, "sqlite refused: {detail}"),
            Self::InvalidEvent { detail } => {
                write!(
                    formatter,
                    "the log holds a row that is not an event: {detail}"
                )
            }
            Self::TaskIdsExhausted { next } => write!(
                formatter,
                "the task id counter reached {next}, which no longer fits FRK- and six digits"
            ),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite {
            detail: error.to_string(),
        }
    }
}

impl From<std::io::Error> for StoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Io {
            detail: error.to_string(),
        }
    }
}

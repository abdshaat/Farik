//! Transcripts recorded from `claude` 2.1.280 with `--model haiku` on 2026-09-22, trimmed of
//! hook lines, thinking signatures, and machine paths. For tests, in this crate and in others.

use super::Transcript;

/// A Read of `/workspace/note.txt`, its result, the text `hello fixture`, and a successful end.
#[must_use]
pub fn reads_a_file() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/reads_a_file.jsonl"))
}

/// A Write refused by a safety check, the model's reply, and a successful end.
#[must_use]
pub fn write_denied() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/write_denied.jsonl"))
}

/// A Read call, then the end a one-turn limit forces.
#[must_use]
pub fn hits_the_turn_limit() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/hits_the_turn_limit.jsonl"))
}

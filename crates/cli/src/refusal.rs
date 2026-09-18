//! The governor's refusals in the words a person reads.
//!
//! `farik-core` answers with values rather than sentences, so that one rule has one answer and every
//! caller says it its own way (`docs/standards/code.md`). This is the command line's way of saying
//! them; the daemon and the app will have their own, and every one of them points at the rule in
//! `docs/SPEC.md` that decided it.

use farik_protocol::event::EventError;

/// Why an event could not be built. Either is this program disagreeing with itself rather than
/// anything the person did, so each says which field was missing.
#[must_use]
pub fn event(error: &EventError) -> String {
    match error {
        // The field is `team_id`, `project_id`, or the body's own field naming who acted, so the
        // sentence names it rather than guessing which of the three it was.
        EventError::BlankId { field } => format!(
            "this event needed a {field} and it is blank, and a blank id names nobody: that is a bug \
             in Farik rather than anything you did"
        ),
        EventError::NoContractNamed { kind } => format!(
            "a {kind} event is about one contract and this one names none, which is a bug in Farik \
             rather than anything you did"
        ),
    }
}

#[cfg(test)]
mod tests {
    use farik_protocol::event::{EventError, EventKind};

    use super::event;

    #[test]
    fn says_which_id_an_event_was_missing() {
        assert_eq!(
            [
                event(&EventError::BlankId {
                    field: "team_id".to_string()
                }),
                event(&EventError::NoContractNamed {
                    kind: EventKind::ContractLocked
                }),
            ],
            [
                "this event needed a team_id and it is blank, and a blank id names nobody: that is \
                 a bug in Farik rather than anything you did",
                "a contract.locked event is about one contract and this one names none, which is a \
                 bug in Farik rather than anything you did",
            ]
        );
    }
}
